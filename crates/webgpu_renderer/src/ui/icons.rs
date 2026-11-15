//! The C++ editor's stroke icons on a 24-unit grid: drawn, not loaded, so no icon font and no licence row.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_4, PI};

use egui::{
    vec2, Color32, Id, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui,
};

use super::palette as pal;
use super::widgets::frame_height;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StudioIcon {
    Cube,
    Camera,
    Sun,
    Mountain,
    Cloud,
    Eye,
    EyeOff,
    Refresh,
    Gauge,
    Keyboard,
    Layers,
}

impl StudioIcon {
    pub const ALL: [Self; 11] = [
        Self::Cube,
        Self::Camera,
        Self::Sun,
        Self::Mountain,
        Self::Cloud,
        Self::Eye,
        Self::EyeOff,
        Self::Refresh,
        Self::Gauge,
        Self::Keyboard,
        Self::Layers,
    ];
}

/// Line segments an arc or curve is cut into.
const SEGMENTS: usize = 12;

/// Grid coordinates to points inside one icon's square.
struct Grid {
    origin: Pos2,
    unit: f32,
    stroke: Stroke,
}

impl Grid {
    fn at(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.unit
    }

    fn path(&self, points: &[(f32, f32)]) -> Vec<Pos2> {
        points.iter().map(|&(x, y)| self.at(x, y)).collect()
    }

    fn closed(&self, points: &[(f32, f32)]) -> Shape {
        Shape::closed_line(self.path(points), self.stroke)
    }

    fn open(&self, points: &[(f32, f32)]) -> Shape {
        Shape::line(self.path(points), self.stroke)
    }

    fn circle(&self, x: f32, y: f32, radius: f32) -> Shape {
        Shape::circle_stroke(self.at(x, y), radius * self.unit, self.stroke)
    }

    /// ImGui's `PathArcTo`: from `a0` to `a1` in radians, y down.
    fn arc(&self, (x, y): (f32, f32), radius: f32, a0: f32, a1: f32) -> Vec<Pos2> {
        (0..=SEGMENTS)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / SEGMENTS as f32;
                self.at(x + radius * a.cos(), y + radius * a.sin())
            })
            .collect()
    }

    fn quadratic(&self, from: (f32, f32), control: (f32, f32), to: (f32, f32)) -> Vec<Pos2> {
        let (p0, p1, p2) = (
            self.at(from.0, from.1),
            self.at(control.0, control.1),
            self.at(to.0, to.1),
        );
        (0..=SEGMENTS)
            .map(|i| {
                let t = i as f32 / SEGMENTS as f32;
                let u = 1.0 - t;
                (p0.to_vec2() * u * u + p1.to_vec2() * 2.0 * u * t + p2.to_vec2() * t * t).to_pos2()
            })
            .collect()
    }
}

/// The icon's strokes filling the square `rect`, about as heavy as Roboto Medium's stems.
pub fn shapes(icon: StudioIcon, rect: Rect, colour: Color32, pixels_per_point: f32) -> Vec<Shape> {
    let unit = rect.width().min(rect.height()) / 24.0;
    let grid = Grid {
        origin: rect.min,
        unit,
        stroke: Stroke::new((2.4 * unit).max(1.0 / pixels_per_point), colour),
    };
    match icon {
        StudioIcon::Cube => cube(&grid),
        StudioIcon::Camera => vec![
            grid.closed(&[
                (3.0, 7.0),
                (7.0, 7.0),
                (9.0, 4.0),
                (15.0, 4.0),
                (17.0, 7.0),
                (21.0, 7.0),
                (21.0, 19.0),
                (3.0, 19.0),
            ]),
            grid.circle(12.0, 13.0, 4.0),
        ],
        StudioIcon::Sun => sun(&grid),
        StudioIcon::Mountain => vec![grid.closed(&[
            (2.0, 19.0),
            (8.5, 6.0),
            (13.0, 13.0),
            (16.0, 9.5),
            (22.0, 19.0),
        ])],
        StudioIcon::Cloud => cloud(&grid),
        StudioIcon::Eye | StudioIcon::EyeOff => eye(&grid, icon == StudioIcon::EyeOff),
        StudioIcon::Refresh => refresh(&grid),
        StudioIcon::Gauge => vec![
            Shape::line(grid.arc((12.0, 14.0), 9.0, PI * 0.8, PI * 2.2), grid.stroke),
            grid.open(&[(12.0, 14.0), (16.0, 9.0)]),
        ],
        StudioIcon::Keyboard => keyboard(&grid),
        StudioIcon::Layers => vec![
            grid.closed(&[(12.0, 4.5), (21.0, 9.5), (12.0, 14.5), (3.0, 9.5)]),
            grid.open(&[(3.0, 14.5), (12.0, 19.5), (21.0, 14.5)]),
        ],
    }
}

fn cube(grid: &Grid) -> Vec<Shape> {
    vec![
        grid.closed(&[
            (12.0, 2.0),
            (21.0, 7.0),
            (21.0, 17.0),
            (12.0, 22.0),
            (3.0, 17.0),
            (3.0, 7.0),
        ]),
        grid.open(&[(3.0, 7.0), (12.0, 12.0), (21.0, 7.0)]),
        grid.open(&[(12.0, 12.0), (12.0, 22.0)]),
    ]
}

fn sun(grid: &Grid) -> Vec<Shape> {
    let mut shapes = vec![grid.circle(12.0, 12.0, 4.0)];
    shapes.extend((0..8).map(|ray| {
        let (sin, cos) = (ray as f32 * FRAC_PI_4).sin_cos();
        grid.open(&[
            (12.0 + cos * 7.5, 12.0 + sin * 7.5),
            (12.0 + cos * 11.0, 12.0 + sin * 11.0),
        ])
    }));
    shapes
}

fn cloud(grid: &Grid) -> Vec<Shape> {
    let mut outline = grid.arc((7.0, 15.0), 4.0, PI * 0.5, PI * 1.5);
    outline.extend(grid.arc((12.0, 11.0), 5.0, PI, PI * 2.0));
    outline.extend(grid.arc((17.0, 15.0), 4.0, PI * 1.5, PI * 2.5));
    vec![Shape::closed_line(outline, grid.stroke)]
}

/// The lens of two curves; the slash replaces the pupil, which would merge with it into a blob.
fn eye(grid: &Grid, crossed: bool) -> Vec<Shape> {
    let mut lens = grid.quadratic((2.0, 12.0), (12.0, 2.0), (22.0, 12.0));
    lens.extend(
        grid.quadratic((22.0, 12.0), (12.0, 22.0), (2.0, 12.0))
            .into_iter()
            .skip(1),
    );
    lens.pop();
    let mark = if crossed {
        grid.open(&[(4.0, 4.0), (20.0, 20.0)])
    } else {
        grid.circle(12.0, 12.0, 3.0)
    };
    vec![Shape::closed_line(lens, grid.stroke), mark]
}

fn refresh(grid: &Grid) -> Vec<Shape> {
    let tip = (12.0 + 8.0 * FRAC_1_SQRT_2, 12.0 - 8.0 * FRAC_1_SQRT_2);
    vec![
        Shape::line(
            grid.arc((12.0, 12.0), 8.0, -PI * 0.25, PI * 1.35),
            grid.stroke,
        ),
        grid.open(&[(tip.0, tip.1 - 5.0), tip, (tip.0 - 5.0, tip.1)]),
    ]
}

fn keyboard(grid: &Grid) -> Vec<Shape> {
    let body = Rect::from_min_max(grid.at(2.0, 6.0), grid.at(22.0, 18.0));
    let mut shapes = vec![Shape::rect_stroke(
        body,
        2.0 * grid.unit,
        grid.stroke,
        StrokeKind::Middle,
    )];
    shapes.extend((0..4).map(|key| {
        let x = 6.0 + 4.0 * key as f32;
        grid.open(&[(x, 10.0), (x + 0.5, 10.0)])
    }));
    shapes.push(grid.open(&[(7.0, 14.0), (17.0, 14.0)]));
    shapes
}

pub fn paint(painter: &Painter, icon: StudioIcon, rect: Rect, colour: Color32) {
    painter.extend(shapes(icon, rect, colour, painter.pixels_per_point()));
}

/// An icon laid out like a glyph of `size` points.
pub fn icon_text(ui: &mut Ui, icon: StudioIcon, size: f32, colour: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    paint(ui.painter(), icon, rect, colour);
    response
}

/// A frame-height square button with a text-size icon: muted, bright under the pointer.
pub fn icon_button(ui: &mut Ui, icon: StudioIcon, tooltip: &str) -> Response {
    let side = frame_height(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
    icon_button_at(
        ui,
        ui.scope_id().with(("icon_button", icon)),
        rect,
        icon,
        true,
        tooltip,
    )
}

/// [`icon_button`] at a rect the caller has laid out, with no fill at rest.
pub fn icon_button_at(
    ui: &Ui,
    id: Id,
    rect: Rect,
    icon: StudioIcon,
    enabled: bool,
    tooltip: &str,
) -> Response {
    let sense = if enabled && ui.is_enabled() {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, id, sense);
    let painter = ui.painter();
    let hot = sense == Sense::click() && response.hovered();
    if hot {
        let fill = if response.is_pointer_button_down_on() {
            pal::FRAME_ACTIVE
        } else {
            pal::FRAME_HOVER
        };
        painter.rect_filled(rect, crate::ui::theme::FRAME_RADIUS, fill);
    }
    let size = super::widgets::line_height(ui);
    let inset = ((rect.width() - size) * 0.5).round();
    let square = Rect::from_min_size(rect.min + vec2(inset, inset), vec2(size, size));
    let colour = match (enabled, hot) {
        (false, _) => pal::TEXT_DISABLED,
        (true, true) => pal::TEXT,
        (true, false) => pal::TEXT_MUTED,
    };
    paint(painter, icon, square, colour);
    response.on_hover_text(tooltip)
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    fn points(shape: &Shape) -> Vec<Pos2> {
        match shape {
            Shape::Path(path) => path.points.clone(),
            Shape::LineSegment { points, .. } => points.to_vec(),
            Shape::Circle(circle) => vec![
                circle.center - vec2(circle.radius, circle.radius),
                circle.center + vec2(circle.radius, circle.radius),
            ],
            Shape::Rect(rect) => vec![rect.rect.min, rect.rect.max],
            other => panic!("unexpected icon shape {other:?}"),
        }
    }

    #[test]
    fn every_icon_draws_inside_its_square_with_the_cpp_stroke() {
        let square = Rect::from_min_size(pos2(100.0, 40.0), vec2(15.0, 15.0));
        for icon in StudioIcon::ALL {
            let shapes = shapes(icon, square, pal::TEXT_MUTED, 1.0);
            assert!(!shapes.is_empty(), "{icon:?}");
            for shape in &shapes {
                for point in points(shape) {
                    assert!(
                        square.expand(0.01).contains(point),
                        "{icon:?} leaves its square at {point:?}"
                    );
                }
            }
        }
        // 2.4 grid units of a 15-point icon; a 1-pixel floor at tiny sizes.
        let Shape::Path(hexagon) = &shapes(StudioIcon::Cube, square, pal::TEXT, 1.0)[0] else {
            panic!("the cube is a path");
        };
        assert_eq!(hexagon.stroke.width, 1.5);
        assert!(hexagon.closed);
        let tiny = Rect::from_min_size(Pos2::ZERO, vec2(4.0, 4.0));
        let Shape::Circle(sun) = &shapes(StudioIcon::Sun, tiny, pal::TEXT, 2.0)[0] else {
            panic!("the sun starts with its disc");
        };
        assert_eq!(sun.stroke.width, 0.5);
    }

    #[test]
    fn an_icon_glyph_takes_its_size_and_the_button_a_frame() {
        let ctx = crate::ui::widgets::test_support::context();
        let (glyph, button) = crate::ui::widgets::test_support::frame(&ctx, Vec::new(), |ui| {
            let glyph = icon_text(ui, StudioIcon::Camera, 18.0, pal::TEXT_MUTED)
                .rect
                .size();
            (
                glyph,
                icon_button(ui, StudioIcon::Refresh, "Rescan").rect.size(),
            )
        });
        assert_eq!(glyph, vec2(18.0, 18.0));
        assert_eq!(button, vec2(23.0, 23.0));
    }
}
