//! The glTF 2.0 accessor rules gltf 1.4.1's reader assumes but never checks: it panics on a file that breaks them.
//! An attribute may take any format KHR_mesh_quantization adds, whether or not the file declares it.

use anyhow::{ensure, Context};
use gltf::accessor::{DataType, Dimensions};
use gltf::mesh::Semantic;
use DataType::{F32, I16, I8, U16, U32, U8};
use Dimensions::{Mat4, Scalar, Vec2, Vec3, Vec4};

/// Every accessor a primitive reads has a format its semantic allows, at least one element, and fits its view.
pub(crate) fn check_primitive_accessors(document: &gltf::Document) -> anyhow::Result<()> {
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let at = || format!("meshes[{}].primitives[{}]", mesh.index(), primitive.index());
            for (semantic, accessor) in primitive.attributes() {
                if let Some(rule) = attribute_rule(&semantic) {
                    check(&accessor, rule).with_context(|| format!("{}.{semantic:?}", at()))?;
                }
            }
            if let Some(indices) = primitive.indices() {
                check(&indices, INDICES).with_context(|| format!("{}.indices", at()))?;
            }
            for target in primitive.morph_targets() {
                let deltas = [
                    (target.positions(), MORPH_POSITIONS),
                    (target.normals(), MORPH_DIRECTIONS),
                    (target.tangents(), MORPH_DIRECTIONS),
                ];
                for (accessor, rule) in deltas {
                    if let Some(accessor) = accessor {
                        check(&accessor, rule).with_context(|| format!("{} morph target", at()))?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Every skin's inverse bind matrices are float MAT4s that fit their view: `load_flat` poses skins with them.
pub(crate) fn check_skin_accessors(document: &gltf::Document) -> anyhow::Result<()> {
    for skin in document.skins() {
        if let Some(accessor) = skin.inverse_bind_matrices() {
            check(&accessor, INVERSE_BINDS)
                .with_context(|| format!("skins[{}].inverseBindMatrices", skin.index()))?;
        }
    }
    Ok(())
}

/// The formats an accessor may have; `unit` when an integer one must be `normalized`, as a direction's must.
#[derive(Clone, Copy)]
struct Rule {
    formats: &'static [(DataType, Dimensions)],
    unit: bool,
}

const fn any(formats: &'static [(DataType, Dimensions)]) -> Rule {
    Rule {
        formats,
        unit: false,
    }
}

const fn unit(formats: &'static [(DataType, Dimensions)]) -> Rule {
    Rule {
        formats,
        unit: true,
    }
}

const POSITIONS: Rule = any(&[
    (F32, Vec3),
    (I8, Vec3),
    (U8, Vec3),
    (I16, Vec3),
    (U16, Vec3),
]);
const NORMALS: Rule = unit(&[(F32, Vec3), (I8, Vec3), (I16, Vec3)]);
const TANGENTS: Rule = unit(&[(F32, Vec4), (I8, Vec4), (I16, Vec4)]);
const TEX_COORDS: Rule = any(&[
    (F32, Vec2),
    (I8, Vec2),
    (U8, Vec2),
    (I16, Vec2),
    (U16, Vec2),
]);
const COLORS: Rule = any(&[
    (F32, Vec3),
    (U8, Vec3),
    (U16, Vec3),
    (F32, Vec4),
    (U8, Vec4),
    (U16, Vec4),
]);
const JOINTS: Rule = any(&[(U8, Vec4), (U16, Vec4)]);
const WEIGHTS: Rule = any(&[(F32, Vec4), (U8, Vec4), (U16, Vec4)]);
const INDICES: Rule = any(&[(U8, Scalar), (U16, Scalar), (U32, Scalar)]);
const MORPH_POSITIONS: Rule = any(&[(F32, Vec3), (I8, Vec3), (I16, Vec3)]);
const INVERSE_BINDS: Rule = any(&[(F32, Mat4)]);
/// Morph NORMAL and TANGENT deltas, both vec3.
const MORPH_DIRECTIONS: Rule = unit(&[(F32, Vec3), (I8, Vec3), (I16, Vec3)]);

/// The rules the readers cover; an application-specific attribute is never read.
fn attribute_rule(semantic: &Semantic) -> Option<Rule> {
    match semantic {
        Semantic::Positions => Some(POSITIONS),
        Semantic::Normals => Some(NORMALS),
        Semantic::Tangents => Some(TANGENTS),
        Semantic::TexCoords(_) => Some(TEX_COORDS),
        Semantic::Colors(_) => Some(COLORS),
        Semantic::Joints(_) => Some(JOINTS),
        Semantic::Weights(_) => Some(WEIGHTS),
        #[allow(unreachable_patterns)]
        _ => None,
    }
}

fn check(accessor: &gltf::Accessor, rule: Rule) -> anyhow::Result<()> {
    let format = (accessor.data_type(), accessor.dimensions());
    ensure!(
        rule.formats.contains(&format),
        "accessor {} is {format:?}, which this attribute cannot be",
        accessor.index()
    );
    ensure!(
        !rule.unit || format.0 == F32 || accessor.normalized(),
        "accessor {} is {format:?} but not normalized, which a direction must be",
        accessor.index()
    );
    ensure!(
        accessor.count() > 0,
        "accessor {} has no elements",
        accessor.index()
    );
    let size = accessor.size();
    if let Some(view) = accessor.view() {
        fits(&view, accessor.offset(), accessor.count(), size)
            .with_context(|| format!("accessor {}", accessor.index()))?;
    }
    if let Some(sparse) = accessor.sparse() {
        let count = sparse.count();
        ensure!(
            count > 0,
            "accessor {}'s sparse block has no elements",
            accessor.index()
        );
        let indices = sparse.indices();
        fits(
            &indices.view(),
            indices.offset(),
            count,
            indices.index_type().size(),
        )
        .with_context(|| format!("accessor {}'s sparse indices", accessor.index()))?;
        let values = sparse.values();
        fits(&values.view(), values.offset(), count, size)
            .with_context(|| format!("accessor {}'s sparse values", accessor.index()))?;
    }
    Ok(())
}

/// `count` elements of `size` bytes from `offset`, at the view's stride, end inside the view.
fn fits(view: &gltf::buffer::View, offset: usize, count: usize, size: usize) -> anyhow::Result<()> {
    let stride = view.stride().unwrap_or(size);
    ensure!(
        stride >= size,
        "stride {stride} is below the element size {size}"
    );
    let end = stride
        .checked_mul(count - 1)
        .and_then(|span| span.checked_add(offset))
        .and_then(|start| start.checked_add(size));
    ensure!(
        end.is_some_and(|end| end <= view.length()),
        "{count} elements of {size} bytes at offset {offset} overrun buffer view {} ({} bytes)",
        view.index(),
        view.length()
    );
    Ok(())
}
