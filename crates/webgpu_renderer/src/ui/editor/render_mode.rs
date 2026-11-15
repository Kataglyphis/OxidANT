//! What the editor says about the render mode: the mode a frame draws, a traced pick waiting for its BVH, and the switch.

use egui::{Response, Ui};

use super::menu_bar::RENDER_MODES;
use super::EditorContext;
use crate::render::tracer::{RenderMode, TraceStatus};
use crate::ui::widgets::Segmented;

/// The drawn mode's label, as the outliner, the inspector title and the switch name it.
pub(super) fn drawn_label(cx: &EditorContext<'_>) -> &'static str {
    RENDER_MODES[cx.stats.drawn_mode.index()]
}

/// The picked traced mode raster stands in for, while it does.
fn pending(cx: &EditorContext<'_>) -> Option<RenderMode> {
    let picked = cx.settings.render_mode;
    (picked.is_traced() && cx.stats.drawn_mode != picked).then_some(picked)
}

/// Why a traced pick still draws raster, for the switch's tooltip and the Renderer tab.
pub(super) fn pending_note(cx: &EditorContext<'_>) -> Option<String> {
    let label = RENDER_MODES[pending(cx)?.index()];
    if let Some(why) = &cx.stats.trace_fallback {
        return Some(format!("{label} rasters: {why}."));
    }
    Some(match &cx.stats.trace {
        TraceStatus::Building(progress) => format!(
            "{label} starts once its BVH is built, {:.0} % so far; Raster cancels it.",
            progress.fraction * 100.0
        ),
        TraceStatus::Failed(why) => format!("{label} is unavailable: {why}"),
        TraceStatus::Idle | TraceStatus::Ready(_) => format!("{label} starts with the next frame."),
    })
}

/// The C++ switch over the drawn mode: a pick waiting for its BVH reads Raster, and Raster again drops it.
pub(super) fn mode_switch(
    ui: &mut Ui,
    segmented: Segmented<'_>,
    cx: &mut EditorContext<'_>,
) -> Response {
    let mut index = cx.stats.drawn_mode.index();
    let response = segmented.reselectable(true).show(ui, &mut index);
    if response.changed() {
        cx.settings.render_mode = RenderMode::from_index(index);
    }
    match pending_note(cx) {
        Some(note) => response.on_hover_text(note),
        None => response,
    }
}

/// The C++ chip's label for what is drawn; raster names its path, and standing in for a traced pick says how far its BVH is.
pub(super) fn chip_label(cx: &EditorContext<'_>) -> String {
    let trace = cx.settings.trace;
    let raster = format!("Raster  ·  {}", cx.settings.raster_path.label());
    match cx.stats.drawn_mode {
        RenderMode::RayTraced => "Ray traced".to_owned(),
        RenderMode::PathTraced => {
            let furnace = if trace.furnace.is_some() {
                "  ·  white furnace"
            } else {
                ""
            };
            format!(
                "Path traced  ·  {} spp  ·  {} bounces{furnace}",
                trace.samples_per_pixel, trace.max_bounces
            )
        }
        RenderMode::Raster => match (pending(cx), &cx.stats.trace) {
            (None, _) => raster,
            (Some(_), _) if cx.stats.trace_fallback.is_some() => {
                format!("{raster}  ·  accumulation too large, rastering")
            }
            (Some(_), TraceStatus::Building(progress)) => {
                format!("{raster}  ·  BVH {:.0} %", progress.fraction * 100.0)
            }
            (Some(_), TraceStatus::Failed(_)) => format!("{raster}  ·  no tracer"),
            (Some(_), _) => format!("{raster}  ·  BVH 0 %"),
        },
    }
}

#[cfg(test)]
mod tests {
    use egui::Event;

    use super::*;
    use crate::app::HostCaps;
    use crate::render::forward::RasterPath;
    use crate::render::tracer::{TraceProgress, TraceStage};
    use crate::scene::camera::OrbitCamera;
    use crate::ui::editor::{DeviceInfo, FrameHistory, RendererSettings, SceneStats, SceneStatus};
    use crate::ui::widgets::test_support::{click, context, frame};

    /// What a host hands the editor, with `picked` chosen and `drawn` drawn.
    struct Host {
        settings: RendererSettings,
        camera: OrbitCamera,
        fly_speed: f32,
        stats: SceneStats,
        frames: FrameHistory,
        device: DeviceInfo,
    }

    impl Host {
        fn new(picked: RenderMode, drawn: RenderMode, trace: TraceStatus) -> Self {
            Self {
                settings: RendererSettings {
                    render_mode: picked,
                    ..RendererSettings::default()
                },
                camera: OrbitCamera::default(),
                fly_speed: 10.0,
                stats: SceneStats {
                    drawn_mode: drawn,
                    trace,
                    ..SceneStats::default()
                },
                frames: FrameHistory::default(),
                device: DeviceInfo::default(),
            }
        }

        fn with<R>(&mut self, use_cx: impl FnOnce(&mut EditorContext<'_>) -> R) -> R {
            let mut cx = EditorContext {
                settings: &mut self.settings,
                camera: &mut self.camera,
                fly_speed: &mut self.fly_speed,
                stats: &self.stats,
                frames: &self.frames,
                device: &self.device,
                model: None,
                scene: SceneStatus::empty(),
                caps: HostCaps::default(),
                surface_px: [800, 600],
            };
            use_cx(&mut cx)
        }
    }

    fn building(fraction: f32) -> TraceStatus {
        TraceStatus::Building(TraceProgress {
            stage: TraceStage::Bvh,
            fraction,
        })
    }

    #[test]
    fn the_chip_names_what_is_drawn_and_how_far_a_waiting_pick_is() {
        let (raster, ray, path) = (
            RenderMode::Raster,
            RenderMode::RayTraced,
            RenderMode::PathTraced,
        );
        let label =
            |picked, drawn, trace| Host::new(picked, drawn, trace).with(|cx| chip_label(cx));
        assert_eq!(
            label(raster, raster, TraceStatus::Idle),
            "Raster  ·  Forward"
        );
        assert_eq!(
            label(path, raster, building(0.424)),
            "Raster  ·  Forward  ·  BVH 42 %"
        );
        let failed = TraceStatus::Failed("too big".into());
        assert_eq!(
            label(ray, raster, failed),
            "Raster  ·  Forward  ·  no tracer"
        );
        assert_eq!(label(ray, ray, TraceStatus::Idle), "Ray traced");
        let deferred = |picked, drawn, trace| {
            let mut host = Host::new(picked, drawn, trace);
            host.settings.raster_path = RasterPath::Deferred;
            host.with(|cx| chip_label(cx))
        };
        assert_eq!(
            deferred(raster, raster, TraceStatus::Idle),
            "Raster  ·  Deferred"
        );
        assert_eq!(
            deferred(path, raster, building(0.424)),
            "Raster  ·  Deferred  ·  BVH 42 %"
        );
        assert_eq!(
            deferred(ray, ray, TraceStatus::Idle),
            "Ray traced",
            "a traced frame shades neither way"
        );
        assert_eq!(
            label(path, path, TraceStatus::Idle),
            "Path traced  ·  8 spp  ·  8 bounces"
        );
    }

    #[test]
    fn a_waiting_pick_says_why_raster_draws_and_a_drawn_one_says_nothing() {
        let mut waiting = Host::new(RenderMode::PathTraced, RenderMode::Raster, building(0.5));
        let note = waiting.with(|cx| pending_note(cx)).expect("a note");
        assert_eq!(
            note,
            "Path traced starts once its BVH is built, 50 % so far; Raster cancels it."
        );
        let mut drawn = Host::new(
            RenderMode::PathTraced,
            RenderMode::PathTraced,
            building(1.0),
        );
        assert_eq!(drawn.with(|cx| pending_note(cx)), None);
        assert_eq!(drawn.with(|cx| drawn_label(cx)), "Path traced");
    }

    #[test]
    fn a_pick_whose_accumulation_does_not_fit_says_it_rasters_and_why() {
        let ready = TraceStatus::Ready(Default::default());
        let mut host = Host::new(RenderMode::PathTraced, RenderMode::Raster, ready);
        host.stats.trace_fallback =
            Some("a 9000 x 9000 accumulation needs 1296000000 bytes".into());
        assert_eq!(
            host.with(|cx| chip_label(cx)),
            "Raster  ·  Forward  ·  accumulation too large, rastering"
        );
        assert_eq!(
            host.with(|cx| pending_note(cx)).as_deref(),
            Some("Path traced rasters: a 9000 x 9000 accumulation needs 1296000000 bytes.")
        );
    }

    /// The switch over 300 points; returns the pick after a frame that clicks `segment`, if any.
    fn switch(host: &mut Host, segment: Option<usize>) -> RenderMode {
        let ctx = context();
        let show = |ui: &mut egui::Ui, host: &mut Host| {
            host.with(|cx| mode_switch(ui, Segmented::new(&RENDER_MODES).width(300.0), cx))
                .rect
        };
        let rect = frame(&ctx, Vec::new(), |ui| show(ui, host));
        let events: Vec<Event> = segment.map_or_else(Vec::new, |index| {
            let x = rect.left() + rect.width() * (index as f32 + 0.5) / 3.0;
            click(x, rect.center().y)
        });
        frame(&ctx, events, |ui| show(ui, host));
        host.settings.render_mode
    }

    #[test]
    fn the_switch_shows_raster_for_a_waiting_pick_keeps_it_and_drops_it_on_raster() {
        let mut host = Host::new(RenderMode::PathTraced, RenderMode::Raster, building(0.5));
        assert_eq!(switch(&mut host, None), RenderMode::PathTraced);
        assert_eq!(switch(&mut host, Some(0)), RenderMode::Raster);
        let mut host = Host::new(RenderMode::Raster, RenderMode::Raster, TraceStatus::Idle);
        assert_eq!(switch(&mut host, Some(1)), RenderMode::RayTraced);
    }
}
