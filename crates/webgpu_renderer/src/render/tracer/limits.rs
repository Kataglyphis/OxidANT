//! The tracer's buffers and texture arrays against the device's limits: WebGPU's defaults in the browser, the adapter's natively.

use super::scene::TraceScene;

/// Storage buffers the scene binds per stage; WebGPU's default of 8 leaves a kernel 4 of its own.
pub const STORAGE_BUFFERS: u32 = 4;
/// Sampled textures and samplers the scene binds per stage.
pub const SAMPLED_TEXTURES: u32 = 2;
pub const SAMPLERS: u32 = 2;

/// Bytes of the four storage buffers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceBufferSizes {
    pub records: u64,
    pub triangles: u64,
    pub positions: u64,
    pub vertices: u64,
}

impl TraceBufferSizes {
    pub fn of(scene: &TraceScene) -> Self {
        Self {
            records: u64::from(scene.layout().records) * 16,
            triangles: scene.triangles.len() as u64 * 16,
            positions: scene.positions.len() as u64 * 16,
            vertices: scene.vertices.len() as u64 * 32,
        }
    }

    /// The buffers by the names the errors and the docs use.
    pub fn named(&self) -> [(&'static str, u64); 4] {
        [
            ("records (nodes, instances, materials)", self.records),
            ("triangles", self.triangles),
            ("positions", self.positions),
            ("vertices", self.vertices),
        ]
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
    let binding = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size);
    for (name, bytes) in TraceBufferSizes::of(scene).named() {
        if bytes > binding {
            return Err(TraceLimitError::Buffer {
                name,
                bytes,
                limit: binding,
            });
        }
    }
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

    #[test]
    fn the_largest_buffer_past_the_browser_binding_limit_is_named() {
        // 4.5 M vertices at 32 bytes is 137 MiB, past WebGPU's 128 MiB per storage binding.
        let scene = sized(4_500_000, 10);
        let error = check(&scene, &wgpu::Limits::default()).unwrap_err();
        assert!(
            matches!(
                error,
                TraceLimitError::Buffer {
                    name: "vertices",
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
        assert_eq!(check(&scene, &device_limits(&adapter, false)), Ok(()));
        assert!(
            check(&scene, &device_limits(&adapter, true)).is_err(),
            "the browser keeps the defaults"
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
    fn the_scene_leaves_a_kernel_half_the_browsers_storage_buffers() {
        let defaults = wgpu::Limits::default();
        assert_eq!(
            defaults.max_storage_buffers_per_shader_stage - STORAGE_BUFFERS,
            4
        );
        assert!(SAMPLED_TEXTURES < defaults.max_sampled_textures_per_shader_stage);
        assert!(SAMPLERS < defaults.max_samplers_per_shader_stage);
    }
}
