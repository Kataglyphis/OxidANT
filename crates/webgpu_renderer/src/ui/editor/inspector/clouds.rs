//! The Sky tab's CLOUDS section: the C++ switch and its nine controls, with the C++ ranges, formats and tooltips.

use egui::Ui;

use crate::render::clouds::{
    CloudSettings, DENSITY_RANGE, LIGHT_STEPS, MARCH_STEPS, OFFSET_RANGE, SCALE_RANGE,
};
use crate::ui::widgets::{
    drag_axes, drag_axes_logarithmic, enabled_ui, property_row, section_header, toggle, FillSlider,
};

/// The C++ drag steps: Scale crosses its range in about 400 px on a log scale, Offset moves a unit per pixel.
const SCALE_STEP: f64 = 2.5;
const OFFSET_STEP: f64 = 1.0;

pub(super) fn show(ui: &mut Ui, clouds: &mut CloudSettings) {
    section_header(ui, "CLOUDS", Some(&mut clouds.enabled));
    let enabled = clouds.enabled;
    enabled_ui(ui, enabled, |ui| {
        steps(ui, clouds);
        shape(ui, clouds);
        placement(ui, clouds);
    });
}

fn steps(ui: &mut Ui, clouds: &mut CloudSettings) {
    let tooltip = "Samples along each view ray through the cloud box (cloud.num_march_steps)";
    property_row(ui, "March steps", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut clouds.march_steps, MARCH_STEPS))
    });
    let tooltip = "Samples from each view sample toward the sun (cloud.num_march_steps_to_light)";
    property_row(ui, "Light steps", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut clouds.light_steps, LIGHT_STEPS))
    });
}

fn shape(ui: &mut Ui, clouds: &mut CloudSettings) {
    let tooltip = "Multiplies every noise sample (cloud.scale)";
    property_row(ui, "Density", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut clouds.density, DENSITY_RANGE))
    });
    let tooltip = "Noise below this threshold is clear sky (cloud.threshold)";
    property_row(ui, "Coverage", Some(tooltip), |ui| {
        ui.add(FillSlider::new(&mut clouds.coverage, 0.0..=1.0))
    });
    property_row(ui, "Pillowness", Some("cloud.pillowness"), |ui| {
        ui.add(FillSlider::new(&mut clouds.pillowness, 0.0..=1.0))
    });
    property_row(ui, "Cirrus", Some("cloud.cirrus_effect"), |ui| {
        ui.add(FillSlider::new(&mut clouds.cirrus, 0.0..=1.0))
    });
    property_row(ui, "Powder effect", Some("cloud.powder_effect"), |ui| {
        toggle(ui, &mut clouds.powder)
    });
}

fn placement(ui: &mut Ui, clouds: &mut CloudSettings) {
    let tooltip = "Extent of the cloud box (cloud.radius)";
    property_row(ui, "Scale", Some(tooltip), |ui| {
        let mut scale = clouds.scale.to_array();
        let response = drag_axes_logarithmic(ui, &mut scale, SCALE_STEP, SCALE_RANGE, 1);
        clouds.scale = scale.into();
        response
    });
    let tooltip = "Centre of the cloud box (cloud.offset)";
    property_row(ui, "Offset", Some(tooltip), |ui| {
        let mut offset = clouds.offset.to_array();
        let response = drag_axes(ui, &mut offset, OFFSET_STEP, OFFSET_RANGE, 1, "");
        clouds.offset = offset.into();
        response
    });
}

#[cfg(test)]
mod tests {
    use egui::{pos2, Pos2, Rect};

    use super::*;
    use crate::ui::widgets::test_support::{click, context, frame};
    use crate::ui::widgets::toggle_size;

    /// One pass of the section; returns where its switch is, at the caption line's right end.
    fn section(ctx: &egui::Context, clouds: &mut CloudSettings, events: Vec<egui::Event>) -> Pos2 {
        frame(ctx, events, |ui| {
            let line_top = ui.cursor().top() + ui.spacing().item_spacing.y * 1.5;
            let size = toggle_size(ui);
            let right = ui.max_rect().right();
            show(ui, clouds);
            Rect::from_min_size(pos2(right - size.x, line_top), size).center()
        })
    }

    #[test]
    fn the_switch_turns_the_clouds_on_and_off() {
        let ctx = context();
        let mut clouds = CloudSettings::default();
        let switch = section(&ctx, &mut clouds, Vec::new());
        section(&ctx, &mut clouds, click(switch.x, switch.y));
        assert!(clouds.enabled, "a click on the switch turns the clouds on");
        section(&ctx, &mut clouds, click(switch.x, switch.y));
        assert!(!clouds.enabled, "a second click turns them off");
    }

    #[test]
    fn showing_the_controls_moves_no_value() {
        let ctx = context();
        let shown = CloudSettings {
            enabled: true,
            ..CloudSettings::default()
        };
        let mut clouds = shown;
        for _ in 0..2 {
            section(&ctx, &mut clouds, Vec::new());
        }
        assert_eq!(
            clouds, shown,
            "a clamping or rounding widget moved a default"
        );
    }
}
