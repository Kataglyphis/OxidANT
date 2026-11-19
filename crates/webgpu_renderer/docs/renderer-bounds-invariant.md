# The bounds invariant (WebGPU renderer)

*Origin and path convention: [the crate README](../README.md).*

Eight separate bugs in this renderer were the same bug. This is the rule, so
there is no ninth.

## The rule

> **World bounds must cover every pose the geometry can actually reach — not the
> pose it was authored in.**

Bounds are the *input* to decisions that delete geometry from the frame. If
they describe a pose the GPU is not drawing, the renderer discards things that
are on screen, and the symptom appears nowhere near the cause.

## Why it keeps happening

Every one of these features moves vertices **on the GPU, at draw time**, while
the bookkeeping that describes where those vertices *are* lives on the CPU and
was written before the feature existed:

| Feature | Moves geometry by | Bookkeeping that forgot |
| --- | --- | --- |
| Morph targets | per-frame vertex re-blend | local AABB, scene bounds |
| Skinning | joint matrices in the shader | local AABB (bind pose only) |
| Instancing | per-instance matrix in the shader | per-primitive AABB, scene bounds |
| LOD | swapping the vertex buffer | drew the *un-morphed* simplified buffer |

Each feature's author updated what they were thinking about and missed the
other consumers, because nothing named them in one place. This document does.

## Consumers — everything that reads bounds

Change any of these and you inherit the invariant:

- **Frustum culling** (`Frustum::intersects_aabb`, which decides what the
  camera passes encode, and its GPU copy `in_camera` in `draw_cull.slang`) —
  wrong bounds delete a visible object outright.
- **Shadow-caster culling** (`intersects_aabb_as_caster`, which picks each
  cascade bundle's casters, and its copy `in_cascade`) — wrong bounds delete a
  *shadow*, which looks like a lighting bug, not a culling bug.
- **Cascade fitting** (`update_cascades` via `scene_bounds`) — this one is
  scene-wide: bad bounds misfit *all three* cascades, so unrelated objects lose
  their shadows.
- **LOD selection** (`geometry_for`, via `world_center`).
- **Transparent sort order** (via `world_center`).
- **Occlusion proxy boxes** — the box drawn *is* the AABB.

Note the last three consume `world_center`, which must stay **one metric**. It
was briefly the vertex centroid at upload and the AABB centre afterwards, so a
`set_animation_time(0.0)` that moved nothing could still flip an LOD level and
reorder blending.

## Maintainers — everything that must update bounds

There are exactly three places, and **all three must handle all of it**:

1. `upload_scene` — the initial pose, including joints already posed away from
   the bind pose (a scene with no animations never reaches the animation path).
2. `set_animation_time` — node transforms, skinning, morph weights.
3. `set_instances` — instance transforms, **and `scene_bounds`**, which was the
   eighth bug: the per-primitive AABB was updated and the scene bounds were not.

Helpers exist so this is composition rather than repetition:
`primitive_local_aabb` (covers morph), `widen_bounds_for_skin`,
`instanced_bounds`, `recompute_scene_bounds`.

`recompute_scene_bounds` is deliberately *one function every mutator calls*,
because the bug it fixes was two call sites disagreeing about whose job it was.

## How to be conservative correctly

Bounds may be **too big** — that only costs a little culling efficiency. Bounds
that are **too small delete visible geometry**. So when a pose is not known
exactly, over-cover it:

- **Morph:** each target covers weights from `min(0, w)` to `max(1, w)`, `w` its
  current weight, so a vertex's extremes are its position plus, per target, the
  lower and the higher of its delta scaled by either end. Exact over `[0,1]`; a
  weight outside it (a default of 2, a CUBICSPLINE overshoot) widens its own
  target's range, and the box is rebuilt whenever an animation frame moves the
  weights, since glTF puts no range on them.
- **Skinning:** skin weights are normalised, so a skinned position is a *convex
  combination* of `J_i · v` and therefore lies inside the **union of the
  per-joint boxes**. Union them.
- **Instancing:** union `transform_aabb(instance_i, posed_box)` over instances.

Each of those is a proof, not a fudge factor — which is why none of them needs a
magic epsilon.

### The tile-light grid is conservative

`render/tile_grid.rs` bounds a point or spot light by the screen rect of its range sphere's
perspective silhouette (Mara & McGuire, *2D Polyhedral Bounds of a Clipped,
Perspective-Projected 3D Sphere*, 2013): per screen axis, the two tangents from the eye to
the sphere's view-space circle give the extreme slopes `x / depth`, and the projection maps
the slope rect's corners to NDC. That is exact for any lens whose clip `w` is a positive
multiple of view depth, the docked viewport's remap included; a pixel of padding absorbs
rounding. A sphere reaching the eye plane, or a non-perspective projection, takes the whole
grid. The bound it replaced, the AABB of the centre and the six `pos ± range * axis` points,
under-covered by up to `D / sqrt(D² - r²)` and cut such a light off in tiles that listed
others. `every_point_a_light_reaches_shades_in_a_tile_that_lists_it` pins it: 300 random
cameras, 32 lights each with ranges from 0.01 to 50, 64 points inside each range; the old
bound missed 4009 of 313052 on-screen points.

Because the bound is conservative, the shader's empty tile shades no light. Two cases
bypass the footprint on purpose. A light without a finite positive `range` (glTF's
unbounded light, Blender's default export) is binned into every tile, as a directional
one is. A tile more than `MAX_LIGHTS_PER_TILE` lights overlap stores its true count and
lists none, and the shader shades every light there instead of a truncated list.

## Non-finite input

Bounds are also where bad data does the most damage, because it *spreads*:

- One non-finite vertex NaNs the primitive bounds → the scene bounds → the
  cascade radius → **all three cascade matrices**, so shadows break for every
  object in the scene, not just the bad mesh.
- `Frustum::intersects_aabb` treats a box with a NaN corner as unknown bounds
  and *visible*, as the C++ engine does, and so does the draw cull's copy
  wherever the GPU keeps IEEE comparisons, so the offending primitive also
  never culls.

Both AABB helpers therefore skip non-finite vertices rather than propagate them.

## Adding a feature that moves geometry

1. Which of the consumers above can now be wrong?
2. Do all three maintainers cover it — including `upload_scene`, for scenes with
   no animation?
3. Is the over-cover argument a proof, or a guess?
4. Write the test against the bounds the frustum test *actually reads*
   (`primitive_world_aabb`, `scene_bounds`), not a recomputed copy.

### One warning about testing this

Several of these bugs are invisible to the obvious test. Confirm your test
**fails before the fix** — twice during this work it did not:

- A cube cannot detect a wrong normal transform under a diagonal scale: its
  normals are axis-aligned, so the correct and incorrect matrices normalise to
  the same direction. Compose the scale with a rotation.
- A camera inside a closed, back-face-culled mesh renders *nothing*, so the
  depth buffer stays empty and an occlusion proxy passes regardless — the strobe
  it was meant to reproduce cannot occur with that geometry.
