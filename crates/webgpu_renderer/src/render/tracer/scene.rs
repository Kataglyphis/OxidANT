//! The traced scene on the CPU: every mesh's BVH and the top level over instances, flattened as the GPU binds them.

use glam::{Mat3, Mat4, Vec3, Vec3A};

use super::bvh::{self, Aabb, Bvh, BvhNode, BvhParams};
use super::geometry::{Gathered, InstanceSource};
use super::layers::LayerArray;
use super::material::{GpuMaterial, MaterialTable};
use crate::render::bounds::transform_aabb;

/// Set in [`GpuInstance::flags`] when the instance mirrors space, which flips its bitangent sign.
pub const INSTANCE_MIRRORED: u32 = 1;

/// One top-level instance: 7 float4 records.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuInstance {
    /// Rows of the world-to-object transform, which carries rays into the mesh.
    pub world_to_object: [[f32; 4]; 3],
    /// Rows of the object-to-world transform.
    pub object_to_world: [[f32; 4]; 3],
    pub blas_root: u32,
    pub flags: u32,
    /// The scene primitive, or `u32::MAX` for the merged world-space mesh.
    pub primitive: u32,
    pub mesh: u32,
}

impl GpuInstance {
    pub const RECORDS: usize = std::mem::size_of::<Self>() / 16;

    fn new(world: Mat4, mesh: &MeshInfo, source: &InstanceSource) -> Self {
        let rows = |m: Mat4| {
            [
                m.row(0).to_array(),
                m.row(1).to_array(),
                m.row(2).to_array(),
            ]
        };
        let mirrored = Mat3::from_mat4(world).determinant() < 0.0;
        Self {
            world_to_object: rows(world.inverse()),
            object_to_world: rows(world),
            blas_root: mesh.root,
            flags: if mirrored { INSTANCE_MIRRORED } else { 0 },
            primitive: source.primitive,
            mesh: source.mesh,
        }
    }

    pub fn world_to_object(&self) -> Mat4 {
        affine(self.world_to_object)
    }

    pub fn object_to_world(&self) -> Mat4 {
        affine(self.object_to_world)
    }
}

fn affine(rows: [[f32; 4]; 3]) -> Mat4 {
    Mat4::from_cols_array_2d(&[rows[0], rows[1], rows[2], [0.0, 0.0, 0.0, 1.0]]).transpose()
}

/// Where one bottom-level mesh sits in the flat arrays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshInfo {
    pub root: u32,
    pub first_triangle: u32,
    pub triangle_count: u32,
    pub first_vertex: u32,
    pub vertex_count: u32,
    /// Object-space bounds.
    pub bounds: Aabb,
}

/// Float4 offsets of the sections in the records buffer: nodes first, then instances, then materials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordLayout {
    pub tlas_root: u32,
    pub instance_base: u32,
    pub material_base: u32,
    pub records: u32,
}

/// What a build produced, for the editor and the logs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceStats {
    /// Triangles over all instances.
    pub triangles: u64,
    /// Triangles stored once per mesh.
    pub unique_triangles: u64,
    pub vertices: u64,
    pub meshes: u32,
    pub instances: u32,
    pub nodes: u32,
    pub materials: u32,
    /// Layers in the sRGB and the linear array, and their sides.
    pub layers: [u32; 2],
    pub layer_sizes: [u32; 2],
    /// Bytes of every buffer and layer the GPU will hold.
    pub gpu_bytes: u64,
    /// Seconds the CPU build took, where the platform has a clock.
    pub build_seconds: Option<f64>,
    /// Of those, the seconds per [`super::TraceStage`]: gathering, BVHs, textures, assembling.
    pub stage_seconds: Option<[f32; 4]>,
}

/// The traced scene in the layout `bvh_traverse.slang` reads; see the module docs for the buffers.
#[derive(Clone, Debug, Default)]
pub struct TraceScene {
    /// Every mesh's nodes, then the top level's.
    pub nodes: Vec<BvhNode>,
    pub blas_node_count: u32,
    /// Three global vertex indices and the material word, in each mesh's leaf order.
    pub triangles: Vec<[u32; 4]>,
    pub positions: Vec<[f32; 4]>,
    /// Packed shading attributes, see `geometry::pack_vertex`.
    pub vertices: Vec<[u32; 8]>,
    /// In top-level leaf order: a leaf's `first` is its instance.
    pub instances: Vec<GpuInstance>,
    pub materials: Vec<GpuMaterial>,
    pub srgb: LayerArray,
    pub linear: LayerArray,
    pub meshes: Vec<MeshInfo>,
    /// Each instance's mesh, primitive and transform below the root, index-aligned with `instances`.
    pub sources: Vec<InstanceSource>,
    pub(crate) root: Mat4,
    pub stats: TraceStats,
}

impl TraceScene {
    /// Flattens the meshes and their hierarchies and builds the top level.
    pub fn assemble(
        table: MaterialTable,
        gathered: Gathered,
        bvhs: Vec<Bvh>,
        layers: [LayerArray; 2],
    ) -> Self {
        let [srgb, linear] = layers;
        let mut scene = Self {
            materials: table.records,
            srgb,
            linear,
            sources: gathered.instances,
            root: Mat4::IDENTITY,
            ..Self::default()
        };
        for (mesh, bvh) in gathered.meshes.into_iter().zip(bvhs) {
            let node_base = scene.nodes.len() as u32;
            let first_triangle = scene.triangles.len() as u32;
            let first_vertex = scene.positions.len() as u32;
            scene.nodes.extend(
                bvh.nodes
                    .iter()
                    .map(|node| relocate(*node, node_base, first_triangle)),
            );
            scene.triangles.extend(bvh.order.iter().map(|&t| {
                let [a, b, c, material] = mesh.triangles[t as usize];
                [
                    a + first_vertex,
                    b + first_vertex,
                    c + first_vertex,
                    material,
                ]
            }));
            scene.meshes.push(MeshInfo {
                root: node_base,
                first_triangle,
                triangle_count: bvh.order.len() as u32,
                first_vertex,
                vertex_count: mesh.positions.len() as u32,
                bounds: bvh.nodes[0].bounds(),
            });
            take_or_append(&mut scene.positions, mesh.positions);
            take_or_append(&mut scene.vertices, mesh.vertices);
        }
        scene.blas_node_count = scene.nodes.len() as u32;
        scene.rebuild_tlas();
        scene
    }

    /// The transform every instance sits under; the editor's model transform.
    pub fn root_transform(&self) -> Mat4 {
        self.root
    }

    /// Moves every instance under `root` and rebuilds the top level; the meshes stay.
    pub fn set_root_transform(&mut self, root: Mat4) {
        self.root = root;
        self.rebuild_tlas();
    }

    /// Moves each instanced primitive to `transforms[primitive]` (an animated node's) and rebuilds the top level; merged ones stay.
    pub fn set_primitive_transforms(&mut self, transforms: &[Mat4]) {
        for source in &mut self.sources {
            if let Some(transform) = transforms.get(source.primitive as usize) {
                source.local = source.extra * *transform;
            }
        }
        self.rebuild_tlas();
    }

    /// Recomputes the instances from their sources and the top level over them.
    pub fn rebuild_tlas(&mut self) {
        self.nodes.truncate(self.blas_node_count as usize);
        let worlds: Vec<Mat4> = self.sources.iter().map(|s| self.root * s.local).collect();
        if worlds.is_empty() {
            self.instances.clear();
            return;
        }
        let boxes: Vec<Aabb> = self
            .sources
            .iter()
            .zip(&worlds)
            .map(|(source, world)| world_bounds(*world, &self.meshes[source.mesh as usize].bounds))
            .collect();
        let tlas = bvh::build(boxes, BvhParams::TLAS);
        self.sources = tlas
            .order
            .iter()
            .map(|&i| self.sources[i as usize])
            .collect();
        self.instances = tlas
            .order
            .iter()
            .zip(&self.sources)
            .map(|(&i, source)| {
                GpuInstance::new(
                    worlds[i as usize],
                    &self.meshes[source.mesh as usize],
                    source,
                )
            })
            .collect();
        let base = self.blas_node_count;
        self.nodes
            .extend(tlas.nodes.iter().map(|node| relocate(*node, base, 0)));
    }

    pub fn tlas_root(&self) -> u32 {
        self.blas_node_count
    }

    pub fn layout(&self) -> RecordLayout {
        let instance_base = (self.nodes.len() * 2) as u32;
        let material_base = instance_base + (self.instances.len() * GpuInstance::RECORDS) as u32;
        RecordLayout {
            tlas_root: self.tlas_root(),
            instance_base,
            material_base,
            records: material_base + (self.materials.len() * GpuMaterial::RECORDS) as u32,
        }
    }

    /// Fills in [`TraceScene::stats`] from the arrays.
    pub fn measure(&mut self, build_seconds: Option<f64>) {
        let triangles = self
            .sources
            .iter()
            .map(|s| u64::from(self.meshes[s.mesh as usize].triangle_count))
            .sum();
        let layout = self.layout();
        let gpu_bytes = u64::from(layout.records) * 16
            + self.triangles.len() as u64 * 16
            + self.positions.len() as u64 * 16
            + self.vertices.len() as u64 * 32
            + self.srgb.byte_size()
            + self.linear.byte_size();
        self.stats = TraceStats {
            triangles,
            unique_triangles: self.triangles.len() as u64,
            vertices: self.positions.len() as u64,
            meshes: self.meshes.len() as u32,
            instances: self.instances.len() as u32,
            nodes: self.nodes.len() as u32,
            materials: self.materials.len() as u32,
            layers: [
                self.srgb.layers.len() as u32,
                self.linear.layers.len() as u32,
            ],
            layer_sizes: [self.srgb.size, self.linear.size],
            gpu_bytes,
            build_seconds,
            stage_seconds: None,
        };
    }
}

/// Shifts a node into the flat arrays: an inner node's children by `node_base`, a leaf's primitives by `first`.
fn relocate(mut node: BvhNode, node_base: u32, first: u32) -> BvhNode {
    node.first += if node.is_leaf() { first } else { node_base };
    node
}

fn take_or_append<T>(all: &mut Vec<T>, mut part: Vec<T>) {
    if all.is_empty() {
        *all = part;
    } else {
        all.append(&mut part);
    }
}

fn world_bounds(world: Mat4, bounds: &Aabb) -> Aabb {
    let (min, max) = transform_aabb(world, Vec3::from(bounds.min), Vec3::from(bounds.max));
    Aabb {
        min: Vec3A::from(min),
        max: Vec3A::from(max),
    }
}
