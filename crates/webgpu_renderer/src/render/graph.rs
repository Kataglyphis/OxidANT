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
    /// Single-sample depth resolved from `DepthMsaa`, written by the G-buffer pass, or the traced primary hit's; SSAO, the cull pass and the clouds read it.
    Depth,
    /// The deferred path's normal, albedo and material targets.
    GBuffer,
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
    /// The clouds' 3D noise, generated on the first frame with clouds and kept.
    NoiseVolume,
    /// Premultiplied cloud colour and coverage, marched to `Depth`.
    CloudColor,
    /// The traced modes' per-pixel running mean.
    Accumulation,
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
            name: "bloom",
            reads: &[Resource::HdrColor, Resource::Exposure],
            writes: &[Resource::Bloom],
        },
        PassDesc {
            name: "ssao",
            reads: &[Resource::Depth],
            writes: &[Resource::Ambient],
        },
        PassDesc {
            name: "cloud_noise",
            reads: &[],
            writes: &[Resource::NoiseVolume],
        },
        PassDesc {
            name: "clouds",
            reads: &[Resource::Depth, Resource::NoiseVolume],
            writes: &[Resource::CloudColor],
        },
        PassDesc {
            name: "tonemap",
            reads: &[
                Resource::HdrColor,
                Resource::Bloom,
                Resource::Ambient,
                Resource::Exposure,
                Resource::CloudColor,
            ],
            writes: &[Resource::Output],
        },
    ]
}

/// The deferred path's maximal frame: the G-buffer and lighting passes replace forward+sky and the resolve.
pub fn deferred_frame_graph() -> Vec<PassDesc<'static>> {
    let mut passes = forward_frame_graph();
    passes.retain(|pass| !matches!(pass.name, "forward+sky" | "depth_resolve"));
    let deferred = [
        PassDesc {
            name: "gbuffer",
            reads: &[],
            writes: &[Resource::GBuffer, Resource::Depth],
        },
        PassDesc {
            name: "deferred_lighting+sky",
            reads: &[Resource::GBuffer, Resource::Depth, Resource::ShadowMap],
            writes: &[Resource::HdrColor],
        },
    ];
    passes.splice(1..1, deferred);
    passes
}

/// A traced frame: `kernel` (`ray_trace` or `path_trace`) and the resolve replace every raster pass before exposure.
/// The kernel writes the primary hit's depth, which the clouds march to as they do to the raster depth.
pub fn traced_frame_graph(kernel: &'static str) -> Vec<PassDesc<'static>> {
    let mut passes = vec![
        PassDesc {
            name: kernel,
            reads: &[],
            writes: &[Resource::Accumulation, Resource::Depth],
        },
        PassDesc {
            name: "trace_resolve",
            reads: &[Resource::Accumulation],
            writes: &[Resource::HdrColor],
        },
    ];
    // SSAO does not run, so the tonemap reads no ambient term.
    let post = forward_frame_graph().into_iter().filter(|pass| {
        matches!(
            pass.name,
            "histogram" | "exposure_reduce" | "bloom" | "cloud_noise" | "clouds"
        )
    });
    passes.extend(post);
    passes.push(PassDesc {
        name: "tonemap",
        reads: &[
            Resource::HdrColor,
            Resource::Bloom,
            Resource::Exposure,
            Resource::CloudColor,
        ],
        writes: &[Resource::Output],
    });
    passes
}

/// Every frame the renderer records: forward, deferred, ray traced, path traced.
#[cfg(test)]
fn every_frame_graph() -> [Vec<PassDesc<'static>>; 4] {
    [
        forward_frame_graph(),
        deferred_frame_graph(),
        traced_frame_graph("ray_trace"),
        traced_frame_graph("path_trace"),
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
        TimedPass::GBuffer => "gbuffer",
        TimedPass::DeferredLighting => "deferred_lighting+sky",
        TimedPass::Clouds => "clouds",
        TimedPass::RayTrace => "ray_trace",
        TimedPass::PathTrace => "path_trace",
        TimedPass::TraceResolve => "trace_resolve",
    }
}

/// Graph rows deliberately left untimed (the cloud noise runs once); not a way to silence the timed-rows test.
#[cfg(test)]
const UNTIMED_ROWS: &[&str] = &["depth_resolve", "cloud_noise"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_graph_is_valid() {
        validate(&forward_frame_graph(), &[]).expect("the shipped frame graph must validate");
    }

    #[test]
    fn deferred_graph_is_valid() {
        validate(&deferred_frame_graph(), &[]).expect("the deferred frame graph must validate");
    }

    #[test]
    fn the_deferred_graph_swaps_only_the_raster_rows() {
        let names = |graph: &[PassDesc<'_>]| graph.iter().map(|p| p.name).collect::<Vec<_>>();
        let deferred = names(&deferred_frame_graph());
        assert_eq!(
            deferred[..3],
            ["shadow", "gbuffer", "deferred_lighting+sky"]
        );
        let forward = names(&forward_frame_graph());
        assert_eq!(
            deferred[3..],
            forward[3..],
            "the post chain is the same in both paths"
        );
        let resolve = |graph: &[PassDesc<'_>]| graph.iter().any(|p| p.writes == [Resource::Depth]);
        assert!(resolve(&forward_frame_graph()) && !resolve(&deferred_frame_graph()));
    }

    #[test]
    fn the_traced_graphs_are_valid() {
        for kernel in ["ray_trace", "path_trace"] {
            validate(&traced_frame_graph(kernel), &[]).expect("a traced frame graph must validate");
        }
    }

    #[test]
    fn every_timed_pass_has_a_graph_row() {
        let graphs = every_frame_graph();
        for pass in TimedPass::ALL {
            let expected = graph_pass_name(pass);
            let in_graph = |graph: &Vec<PassDesc>| graph.iter().any(|p| p.name == expected);
            assert!(
                graphs.iter().any(in_graph),
                "TimedPass::{pass:?} maps to graph row '{expected}', but no frame has such a row"
            );
            let traced_only = !in_graph(&graphs[0]) && !in_graph(&graphs[1]);
            assert_eq!(pass.is_traced(), traced_only, "{pass:?}");
        }
    }

    #[test]
    fn every_graph_row_is_timed_or_explicitly_untimed() {
        let timed_names: Vec<&str> = TimedPass::ALL.iter().map(|&p| graph_pass_name(p)).collect();
        for pass in every_frame_graph().iter().flatten() {
            assert!(
                timed_names.contains(&pass.name) || UNTIMED_ROWS.contains(&pass.name),
                "graph row '{}' is neither timed by any TimedPass nor listed in UNTIMED_ROWS",
                pass.name
            );
        }
    }

    /// The timed passes frame `frame` of [`every_frame_graph`] records: each skips the other paths' and modes' passes.
    fn timed_in(frame: usize) -> Vec<&'static str> {
        use TimedPass::*;
        const RASTER: [TimedPass; 6] = [
            ShadowCascades,
            Forward,
            GBuffer,
            DeferredLighting,
            OcclusionCull,
            Ssao,
        ];
        let skipped: Vec<TimedPass> = match frame {
            0 => vec![GBuffer, DeferredLighting, RayTrace, PathTrace, TraceResolve],
            1 => vec![Forward, RayTrace, PathTrace, TraceResolve],
            2 => [RASTER.as_slice(), &[PathTrace]].concat(),
            _ => [RASTER.as_slice(), &[RayTrace]].concat(),
        };
        let passes = TimedPass::ALL
            .into_iter()
            .filter(|pass| !skipped.contains(pass));
        passes.map(graph_pass_name).collect()
    }

    #[test]
    fn each_frames_graph_has_a_row_for_every_pass_it_times_and_times_every_other_row() {
        for (frame, graph) in every_frame_graph().into_iter().enumerate() {
            let rows: Vec<&str> = graph.iter().map(|pass| pass.name).collect();
            let timed = timed_in(frame);
            let untimed: Vec<&str> = rows
                .iter()
                .copied()
                .filter(|row| !timed.contains(row))
                .collect();
            assert!(
                timed.iter().all(|pass| rows.contains(pass)),
                "frame {frame}: timed {timed:?}, rows {rows:?}"
            );
            assert!(
                untimed.iter().all(|row| UNTIMED_ROWS.contains(row)),
                "frame {frame}: rows {untimed:?} are untimed"
            );
        }
    }

    #[test]
    fn the_clouds_march_after_the_depth_and_before_the_tonemap_in_every_frame() {
        for graph in every_frame_graph() {
            let at = |name: &str| graph.iter().position(|pass| pass.name == name);
            let writer = graph
                .iter()
                .position(|pass| pass.writes.contains(&Resource::Depth))
                .expect("a pass writes the single-sample depth");
            let (noise, clouds, tonemap) = (at("cloud_noise"), at("clouds"), at("tonemap"));
            let (Some(noise), Some(clouds), Some(tonemap)) = (noise, clouds, tonemap) else {
                panic!("a graph lacks the cloud rows");
            };
            assert!(writer < clouds && noise < clouds && clouds < tonemap);
            assert!(graph[tonemap].reads.contains(&Resource::CloudColor));
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
