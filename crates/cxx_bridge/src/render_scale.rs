//! The render scale both renderers share, in the C++ namespace `kataglyphis::oxidant::render_scale`: its defaults and
//! ranges, the scene extent it gives an output, and the Auto controller that steers it, all
//! `kataglyphis_render_core::render_scale`'s.

use kataglyphis_render_core::render_scale::{
    self, RenderScale as CoreRenderScale, RENDER_SCALE_RANGE, SCALE_STEP, TARGET_FPS_RANGE,
};

use crate::{guarded, nothing};

#[cxx::bridge(namespace = "kataglyphis::oxidant::render_scale")]
pub mod ffi {
    /// `RenderScale` field for field; `auto` is a C++ keyword, so Auto's switch is `auto_scale`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct RenderScale {
        scale: f32,
        auto_scale: bool,
        target_fps: f32,
    }

    /// Both ends included.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ScaleRange {
        min: f32,
        max: f32,
    }

    /// The editor's ranges, and the step Auto moves the scale by.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct RenderScaleRanges {
        scale: ScaleRange,
        target_fps: ScaleRange,
        step: f32,
    }

    /// The scene targets' size for an output, and the scale they were taken at after the clamp.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ScaledExtent {
        width: u32,
        height: u32,
        scale: f32,
    }

    /// One frame's measurement: the GPU's pass time where timestamps exist, else the frame time, which vsync caps.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct FrameCost {
        gpu_timed: bool,
        ms: f32,
    }

    /// Auto's verdict on one frame: `scale` is the new one when `changed`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct ScaleChange {
        changed: bool,
        scale: f32,
    }

    extern "Rust" {
        type AutoScaler;

        /// Full size, Auto off, aiming at 60 FPS.
        fn default_render_scale() -> RenderScale;
        fn render_scale_ranges() -> RenderScaleRanges;
        /// Each side of the output scaled and rounded, at least 1; a non-finite scale renders at full size.
        fn render_scale_extent(settings: RenderScale, width: u32, height: u32) -> ScaledExtent;
        /// A controller that has changed nothing yet.
        fn new_auto_scaler() -> Box<AutoScaler>;
        /// Feeds one frame at `now_s` seconds; the caller applies a change to its settings.
        fn update(
            self: &mut AutoScaler,
            settings: RenderScale,
            cost: FrameCost,
            now_s: f64,
        ) -> ScaleChange;
    }
}

/// Cannot panic; `guarded` keeps the bridge's contract all the same, as for every call.
pub fn default_render_scale() -> ffi::RenderScale {
    guarded(|| bridged(CoreRenderScale::default()), nothing)
}

pub fn render_scale_ranges() -> ffi::RenderScaleRanges {
    guarded(
        || ffi::RenderScaleRanges {
            scale: range(RENDER_SCALE_RANGE),
            target_fps: range(TARGET_FPS_RANGE),
            step: SCALE_STEP,
        },
        nothing,
    )
}

pub fn render_scale_extent(
    settings: ffi::RenderScale,
    width: u32,
    height: u32,
) -> ffi::ScaledExtent {
    guarded(
        || {
            let core = core(settings);
            let (width, height) = core.extent((width, height));
            ffi::ScaledExtent {
                width,
                height,
                scale: core.clamped(),
            }
        },
        nothing,
    )
}

pub fn new_auto_scaler() -> Box<AutoScaler> {
    guarded(Box::default, nothing)
}

/// `kataglyphis_render_core`'s controller, held by the C++ app between frames.
#[derive(Default)]
pub struct AutoScaler(render_scale::AutoScaler);

impl AutoScaler {
    pub fn update(
        &mut self,
        settings: ffi::RenderScale,
        cost: ffi::FrameCost,
        now_s: f64,
    ) -> ffi::ScaleChange {
        guarded(
            || {
                let cost = if cost.gpu_timed {
                    render_scale::FrameCost::Gpu(cost.ms)
                } else {
                    render_scale::FrameCost::Frame(cost.ms)
                };
                self.0.update(&core(settings), cost, now_s).map_or_else(
                    ffi::ScaleChange::default,
                    |scale| ffi::ScaleChange {
                        changed: true,
                        scale,
                    },
                )
            },
            nothing,
        )
    }
}

fn core(settings: ffi::RenderScale) -> CoreRenderScale {
    CoreRenderScale {
        scale: settings.scale,
        auto: settings.auto_scale,
        target_fps: settings.target_fps,
    }
}

fn bridged(settings: CoreRenderScale) -> ffi::RenderScale {
    ffi::RenderScale {
        scale: settings.scale,
        auto_scale: settings.auto,
        target_fps: settings.target_fps,
    }
}

fn range(range: std::ops::RangeInclusive<f32>) -> ffi::ScaleRange {
    ffi::ScaleRange {
        min: *range.start(),
        max: *range.end(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kataglyphis_render_core::render_scale::DEFAULT_TARGET_FPS;

    fn auto(scale: f32) -> ffi::RenderScale {
        ffi::RenderScale {
            scale,
            auto_scale: true,
            ..default_render_scale()
        }
    }

    #[test]
    fn the_defaults_and_ranges_are_render_cores() {
        assert_eq!(core(default_render_scale()), CoreRenderScale::default());
        assert_eq!(
            default_render_scale(),
            ffi::RenderScale {
                scale: 1.0,
                auto_scale: false,
                target_fps: DEFAULT_TARGET_FPS
            }
        );
        let ranges = render_scale_ranges();
        assert_eq!(
            ranges.scale,
            ffi::ScaleRange {
                min: 0.25,
                max: 1.0
            }
        );
        assert_eq!(
            ranges.target_fps,
            ffi::ScaleRange {
                min: 20.0,
                max: 144.0
            }
        );
        assert_eq!(ranges.step, 0.05);
    }

    #[test]
    fn the_extent_is_render_cores_with_the_scale_it_took() {
        let half = ffi::RenderScale {
            scale: 0.5,
            ..default_render_scale()
        };
        assert_eq!(
            render_scale_extent(half, 1928, 1207),
            ffi::ScaledExtent {
                width: 964,
                height: 604,
                scale: 0.5
            }
        );
        let tiny = ffi::RenderScale {
            scale: 0.01,
            ..half
        };
        assert_eq!(render_scale_extent(tiny, 100, 100).scale, 0.25);
        let broken = ffi::RenderScale {
            scale: f32::NAN,
            ..half
        };
        let full = render_scale_extent(broken, 640, 360);
        assert_eq!((full.width, full.height, full.scale), (640, 360, 1.0));
    }

    #[test]
    fn the_bridged_scaler_is_render_cores() {
        let mut scaler = new_auto_scaler();
        let over = ffi::FrameCost {
            gpu_timed: false,
            ms: 28.0,
        };
        let change = scaler.update(auto(1.0), over, 0.0);
        assert_eq!(
            change,
            ffi::ScaleChange {
                changed: true,
                scale: 0.75
            }
        );
        assert!(!scaler.update(auto(0.75), over, 0.5).changed, "it waits");
        let broken = ffi::FrameCost {
            gpu_timed: true,
            ms: f32::NAN,
        };
        assert!(!new_auto_scaler().update(auto(0.5), broken, 100.0).changed);
    }
}
