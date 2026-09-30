//! A small render graph: passes declare what they read and write, and `validate` checks the wiring.
//! It only describes the frame the renderer records; order is insertion order, not a scheduler.

#[cfg(test)]
use super::gpu_timing::TimedPass;
use std::collections::HashSet;

/// Named GPU resources the passes exchange within a frame.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Cascaded shadow depth array.
    ShadowMap,
    /// MSAA depth buffer written by the forward pass.
    DepthMsaa,
    /// Single-sample depth resolved from `DepthMsaa`; SSAO and the cull pass's input.
    Depth,
    /// HDR scene color.
    HdrColor,
    /// Cull-pass visibility; the *next* frame's forward pass reads it, so nothing here does.
    Visibility,
    /// Blurred bloom contribution.
    Bloom,
    /// Blurred ambient occlusion factor.
    Ambient,
    /// Luminance histogram bins.
    Histogram,
    /// Adapted exposure value reduced from `Histogram`.
    Exposure,
    /// Final display target (swapchain frame or readback texture).
    Output,
}

/// A declared pass: its name and the resources it reads and writes.
pub struct PassDesc<'a> {
    pub name: &'static str,
    pub reads: &'a [Resource],
    pub writes: &'a [Resource],
}

#[derive(Debug, PartialEq, Eq)]
pub enum GraphError {
    /// A pass reads a resource that no earlier pass wrote.
    UndefinedRead {
        pass: &'static str,
        resource: Resource,
    },
    /// Two passes write the same resource in one frame.
    DuplicateWrite {
        pass: &'static str,
        resource: Resource,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::UndefinedRead { pass, resource } => write!(
                f,
                "pass '{pass}' reads {resource:?} before anything writes it"
            ),
            GraphError::DuplicateWrite { pass, resource } => {
                write!(f, "pass '{pass}' writes {resource:?} twice in one frame")
            }
        }
    }
}

impl std::error::Error for GraphError {}

/// Checks every read follows an earlier write (or is in `external`) and nothing is written twice.
pub fn validate(passes: &[PassDesc<'_>], external: &[Resource]) -> Result<(), GraphError> {
    let mut written: HashSet<Resource> = external.iter().copied().collect();
    for pass in passes {
        for resource in pass.reads {
            if !written.contains(resource) {
                return Err(GraphError::UndefinedRead {
                    pass: pass.name,
                    resource: *resource,
                });
            }
        }
        for resource in pass.writes {
            if !written.insert(*resource) {
                return Err(GraphError::DuplicateWrite {
                    pass: pass.name,
                    resource: *resource,
                });
            }
        }
    }
    Ok(())
}

/// The forward renderer's *maximal* frame as data; passes skipped at runtime stay declared.
pub fn forward_frame_graph() -> Vec<PassDesc<'static>> {
    vec![
        PassDesc {
            name: "shadow",
            reads: &[],
            writes: &[Resource::ShadowMap],
        },
        PassDesc {
            name: "forward+sky",
            reads: &[Resource::ShadowMap],
            writes: &[Resource::HdrColor, Resource::DepthMsaa],
        },
        PassDesc {
            name: "depth_resolve",
            reads: &[Resource::DepthMsaa],
            writes: &[Resource::Depth],
        },
        PassDesc {
            name: "occlusion_cull",
            reads: &[Resource::Depth],
            writes: &[Resource::Visibility],
        },
        PassDesc {
            name: "bloom",
            reads: &[Resource::HdrColor],
            writes: &[Resource::Bloom],
        },
        PassDesc {
            name: "ssao",
            reads: &[Resource::Depth],
            writes: &[Resource::Ambient],
        },
        PassDesc {
            name: "histogram",
            reads: &[Resource::HdrColor],
            writes: &[Resource::Histogram],
        },
        PassDesc {
            name: "exposure_reduce",
            reads: &[Resource::Histogram],
            writes: &[Resource::Exposure],
        },
        PassDesc {
            name: "tonemap",
            reads: &[
                Resource::HdrColor,
                Resource::Bloom,
                Resource::Ambient,
                Resource::Exposure,
            ],
            writes: &[Resource::Output],
        },
    ]
}

/// Graph row of a timed pass; a `match` so a new `TimedPass` variant fails to compile here.
#[cfg(test)]
fn graph_pass_name(pass: TimedPass) -> &'static str {
    match pass {
        TimedPass::ShadowCascades => "shadow",
        TimedPass::Forward => "forward+sky",
        TimedPass::OcclusionCull => "occlusion_cull",
        TimedPass::Bloom => "bloom",
        TimedPass::Ssao => "ssao",
        TimedPass::Histogram => "histogram",
        TimedPass::ExposureReduce => "exposure_reduce",
        TimedPass::Tonemap => "tonemap",
    }
}

/// Graph rows deliberately left untimed; not a way to silence the timed-rows test.
#[cfg(test)]
const UNTIMED_ROWS: &[&str] = &["depth_resolve"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_graph_is_valid() {
        validate(&forward_frame_graph(), &[]).expect("the shipped frame graph must validate");
    }

    #[test]
    fn every_timed_pass_has_a_graph_row() {
        let graph = forward_frame_graph();
        for pass in TimedPass::ALL {
            let expected = graph_pass_name(pass);
            assert!(
                graph.iter().any(|p| p.name == expected),
                "TimedPass::{pass:?} maps to graph row '{expected}', but no such row exists"
            );
        }
    }

    #[test]
    fn every_graph_row_is_timed_or_explicitly_untimed() {
        let graph = forward_frame_graph();
        let timed_names: Vec<&str> = TimedPass::ALL.iter().map(|&p| graph_pass_name(p)).collect();
        for pass in &graph {
            assert!(
                timed_names.contains(&pass.name) || UNTIMED_ROWS.contains(&pass.name),
                "graph row '{}' is neither timed by any TimedPass nor listed in UNTIMED_ROWS",
                pass.name
            );
        }
    }

    #[test]
    fn detects_reading_undefined_resource() {
        let passes = vec![PassDesc {
            name: "tonemap",
            reads: &[Resource::Bloom],
            writes: &[Resource::Output],
        }];
        assert_eq!(
            validate(&passes, &[]),
            Err(GraphError::UndefinedRead {
                pass: "tonemap",
                resource: Resource::Bloom
            })
        );
    }

    #[test]
    fn detects_double_write() {
        let passes = vec![
            PassDesc {
                name: "a",
                reads: &[],
                writes: &[Resource::HdrColor],
            },
            PassDesc {
                name: "b",
                reads: &[],
                writes: &[Resource::HdrColor],
            },
        ];
        assert_eq!(
            validate(&passes, &[]),
            Err(GraphError::DuplicateWrite {
                pass: "b",
                resource: Resource::HdrColor
            })
        );
    }

    #[test]
    fn external_resources_satisfy_reads() {
        let passes = vec![PassDesc {
            name: "overlay",
            reads: &[Resource::Output],
            writes: &[],
        }];
        assert!(validate(&passes, &[Resource::Output]).is_ok());
    }
}
