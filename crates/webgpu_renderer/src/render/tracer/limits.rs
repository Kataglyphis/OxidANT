//! The tracer's buffers and texture arrays against the device's limits: WebGPU's defaults in the browser, the adapter's natively.

use super::material::GpuMaterial;
use super::scene::{GpuInstance, TraceScene};

/// Storage buffers the scene binds per stage; WebGPU's default of 8 leaves a kernel 2 of its own.
pub const STORAGE_BUFFERS: u32 = 6;
/// Sampled textures and samplers the scene binds per stage.
pub const SAMPLED_TEXTURES: u32 = 2;
pub const SAMPLERS: u32 = 2;

/// What a scene stores, which sizes its buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceCounts {
    pub nodes: u64,
    pub instances: u64,
    pub materials: u64,
    pub triangles: u64,
    pub vertices: u64,
}

/// Bytes of the six storage buffers: a node's two halves and a vertex's two halves bind apart, 16 bytes an entry each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceBufferSizes {
    pub records: u64,
    pub node_max: u64,
    pub triangles: u64,
    pub positions: u64,
    pub vertices: u64,
    pub vertex_extras: u64,
}

impl TraceBufferSizes {
    pub fn of(scene: &TraceScene) -> Self {
        Self::of_counts(TraceCounts {
            nodes: scene.nodes.len() as u64,
            instances: scene.instances.len() as u64,
            materials: scene.materials.len() as u64,
            triangles: scene.triangle_count(),
            vertices: scene.vertex_count(),
        })
    }

    pub fn of_counts(counts: TraceCounts) -> Self {
        let records = counts.nodes
            + counts.instances * GpuInstance::RECORDS as u64
            + counts.materials * GpuMaterial::RECORDS as u64;
        Self {
            records: records * 16,
            node_max: counts.nodes * 16,
            triangles: counts.triangles * 16,
            positions: counts.vertices * 16,
            vertices: counts.vertices * 16,
            vertex_extras: counts.vertices * 16,
        }
    }

    /// The buffers by the names the errors and the docs use.
    pub fn named(&self) -> [(&'static str, u64); 6] {
        [
            ("records (node minima, instances, materials)", self.records),
            ("node maxima", self.node_max),
            ("triangles", self.triangles),
            ("positions", self.positions),
            ("vertices (normal, tangent, UV0)", self.vertices),
            ("vertex extras (UV1, colour)", self.vertex_extras),
        ]
    }

    pub fn total(&self) -> u64 {
        self.named().iter().map(|(_, bytes)| bytes).sum()
    }
}

/// Why a scene cannot be traced on a device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceLimitError {
    Buffer {
        name: &'static str,
        bytes: u64,
        limit: u64,
    },
    Layers {
        layers: u32,
        limit: u32,
    },
    LayerSize {
        size: u32,
        limit: u32,
    },
}

impl std::fmt::Display for TraceLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mib = |bytes: u64| bytes as f64 / f64::from(1u32 << 20);
        match self {
            Self::Buffer { name, bytes, limit } => write!(
                f,
                "the tracer's {name} buffer needs {:.1} MiB; this device binds at most {:.1} MiB",
                mib(*bytes),
                mib(*limit)
            ),
            Self::Layers { layers, limit } => {
                write!(
                    f,
                    "the tracer needs {layers} texture layers; this device allows {limit}"
                )
            }
            Self::LayerSize { size, limit } => {
                write!(
                    f,
                    "the tracer's {size}-texel layers exceed this device's {limit}"
                )
            }
        }
    }
}

impl std::error::Error for TraceLimitError {}

/// Checks the buffers against the binding and buffer limits and the arrays against the texture limits.
pub fn check(scene: &TraceScene, limits: &wgpu::Limits) -> Result<(), TraceLimitError> {
    check_buffers(&TraceBufferSizes::of(scene), limits)?;
    for array in [&scene.srgb, &scene.linear] {
        let layers = array.layers.len() as u32;
        if layers > limits.max_texture_array_layers {
            return Err(TraceLimitError::Layers {
                layers,
                limit: limits.max_texture_array_layers,
            });
        }
        if array.size > limits.max_texture_dimension_2d {
            return Err(TraceLimitError::LayerSize {
                size: array.size,
                limit: limits.max_texture_dimension_2d,
            });
        }
    }
    Ok(())
}

/// Checks each buffer against the storage binding and buffer limits; the first one past them is named.
pub fn check_buffers(
    sizes: &TraceBufferSizes,
    limits: &wgpu::Limits,
) -> Result<(), TraceLimitError> {
    let binding = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    match sizes
        .named()
        .into_iter()
        .find(|(_, bytes)| *bytes > binding)
    {
        Some((name, bytes)) => Err(TraceLimitError::Buffer {
            name,
            bytes,
            limit: binding,
        }),
        None => Ok(()),
    }
}

/// The limits to request: WebGPU's defaults, natively with the adapter's storage-binding and buffer sizes so a large scene traces.
pub fn device_limits(adapter: &wgpu::Limits, web: bool) -> wgpu::Limits {
    let mut limits = wgpu::Limits::default();
    if !web {
        limits.max_storage_buffer_binding_size = adapter
            .max_storage_buffer_binding_size
            .max(limits.max_storage_buffer_binding_size);
        limits.max_buffer_size = adapter.max_buffer_size.max(limits.max_buffer_size);
    }
    limits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::tracer::layers::LayerArray;

    /// A scene with `triangles` triangles, as many vertices and `nodes` nodes, the arrays left empty.
    fn sized(triangles: usize, nodes: usize) -> TraceScene {
        TraceScene {
            nodes: vec![Default::default(); nodes],
            triangles: vec![[0; 4]; triangles],
            positions: vec![[0.0; 4]; triangles],
            vertices: vec![[0; 8]; triangles],
            ..TraceScene::default()
        }
    }

    #[test]
    fn a_scene_within_the_browser_defaults_passes_them() {
        let scene = sized(100_000, 120_000);
        assert_eq!(check(&scene, &wgpu::Limits::default()), Ok(()));
    }

    /// The full Colosseum's counts, as `the_full_colosseum_traces_on_this_device` prints them.
    const COLOSSEUM: TraceCounts = TraceCounts {
        nodes: 5_742_715,
        instances: 1113,
        materials: 68,
        triangles: 5_871_293,
        vertices: 4_684_878,
    };

    #[test]
    fn the_full_colosseum_fits_the_browser_binding_limit() {
        let defaults = wgpu::Limits::default();
        let sizes = TraceBufferSizes::of_counts(COLOSSEUM);
        assert_eq!(check_buffers(&sizes, &defaults), Ok(()), "{sizes:?}");
        // Vertices whole in one binding, 32 bytes each, would not fit, nor would the more numerous nodes.
        assert!(COLOSSEUM.vertices * 32 > defaults.max_storage_buffer_binding_size);
    }

    #[test]
    fn the_largest_buffer_past_the_browser_binding_limit_is_named() {
        // 9 M triangles at 16 bytes is 137 MiB, past WebGPU's 128 MiB per storage binding.
        let sizes = TraceBufferSizes::of_counts(TraceCounts {
            triangles: 9_000_000,
            ..COLOSSEUM
        });
        let error = check_buffers(&sizes, &wgpu::Limits::default()).unwrap_err();
        assert!(
            matches!(
                error,
                TraceLimitError::Buffer {
                    name: "triangles",
                    ..
                }
            ),
            "{error}"
        );
        assert!(error.to_string().contains("128.0 MiB"), "{error}");
        let adapter = wgpu::Limits {
            max_storage_buffer_binding_size: 1 << 31,
            max_buffer_size: 1 << 32,
            ..wgpu::Limits::default()
        };
        assert_eq!(
            check_buffers(&sizes, &device_limits(&adapter, false)),
            Ok(())
        );
        assert!(
            check_buffers(&sizes, &device_limits(&adapter, true)).is_err(),
            "the browser keeps the defaults"
        );
    }

    #[test]
    fn a_scene_sizes_its_buffers_by_its_counts() {
        let scene = sized(10, 30);
        let sizes = TraceBufferSizes::of(&scene);
        assert_eq!(sizes.records, u64::from(scene.layout().records) * 16);
        assert_eq!((sizes.node_max, sizes.triangles), (30 * 16, 10 * 16));
        assert_eq!((sizes.vertices, sizes.vertex_extras), (10 * 16, 10 * 16));
        assert_eq!(
            sizes.total(),
            sizes.named().iter().map(|n| n.1).sum::<u64>()
        );
    }

    #[test]
    fn native_limits_take_only_the_adapters_buffer_sizes() {
        let adapter = wgpu::Limits {
            max_storage_buffer_binding_size: 1 << 30,
            max_buffer_size: 1 << 33,
            max_texture_dimension_2d: 16384,
            max_storage_buffers_per_shader_stage: 64,
            ..wgpu::Limits::default()
        };
        let native = device_limits(&adapter, false);
        assert_eq!(native.max_storage_buffer_binding_size, 1 << 30);
        assert_eq!(native.max_buffer_size, 1 << 33);
        assert_eq!(
            native.max_texture_dimension_2d, 8192,
            "everything else stays the web's"
        );
        assert_eq!(native.max_storage_buffers_per_shader_stage, 8);
        assert_eq!(device_limits(&adapter, true), wgpu::Limits::default());
    }

    #[test]
    fn texture_arrays_past_the_layer_or_side_limit_are_refused() {
        let mut scene = sized(10, 10);
        scene.srgb = LayerArray {
            size: 16384,
            layers: vec![Vec::new()],
        };
        assert!(matches!(
            check(&scene, &wgpu::Limits::default()),
            Err(TraceLimitError::LayerSize { .. })
        ));
        scene.srgb = LayerArray {
            size: 4,
            layers: vec![Vec::new(); 300],
        };
        assert!(matches!(
            check(&scene, &wgpu::Limits::default()),
            Err(TraceLimitError::Layers { .. })
        ));
    }

    #[test]
    fn the_scene_leaves_a_kernel_two_of_the_browsers_storage_buffers() {
        let defaults = wgpu::Limits::default();
        assert_eq!(
            defaults.max_storage_buffers_per_shader_stage - STORAGE_BUFFERS,
            2
        );
        assert!(SAMPLED_TEXTURES < defaults.max_sampled_textures_per_shader_stage);
        assert!(SAMPLERS < defaults.max_samplers_per_shader_stage);
    }
}
