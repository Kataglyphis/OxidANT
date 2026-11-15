//! The scene dock's model library, the C++ `renderModelLibrary`: folders and a filter; a click picks, a double-click, Enter or Load loads.

use std::collections::HashSet;

use egui::{
    pos2, vec2, Align, Color32, Key, Margin, Painter, Pos2, Rect, RichText, ScrollArea, Sense,
    Shape, Stroke, TextEdit, TextStyle, Ui,
};

use super::{Editor, EditorAction, EditorContext, SceneStatus};
use crate::app::library::{LibraryEntry, LibraryGroup, ModelLibrary, TextFilter};
use crate::ui::palette as pal;
use crate::ui::theme;
use crate::ui::widgets::{
    ellipsized, frame_height, galley, line_height, list_row, muted_wrapped, RowSpec, RowTrailing,
};

/// The empty library's text: where scenes come from.
const EMPTY: &str = "No scenes found. Fetch them with scripts/linux/fetch-models.sh --scenes, \
                     or drop a .gltf, .glb or .obj on the window.";

/// The library's state across frames.
#[derive(Clone, Debug)]
pub struct LibraryState {
    filter: String,
    /// The last clicked row; the highlight falls back to the loaded row.
    cursor: Option<usize>,
    /// Enter loads the cursor while armed: a row click arms it, any other press or Esc disarms it.
    armed: bool,
    /// Open folders, by their name in lower case, as the C++ groups ignore case.
    open: HashSet<String>,
    /// The loaded row last frame; a new one is scrolled to and its folder opened.
    last_loaded: Option<usize>,
    /// Scroll the loaded row into view when it is next drawn: on first show and when F1 brings the editor back.
    scroll: bool,
}

impl Default for LibraryState {
    fn default() -> Self {
        Self {
            filter: String::new(),
            cursor: None,
            armed: false,
            open: HashSet::new(),
            last_loaded: None,
            scroll: true,
        }
    }
}

impl LibraryState {
    pub fn scroll_to_loaded(&mut self) {
        self.scroll = true;
    }

    /// A newly loaded row is scrolled to and its folder opened.
    fn follow(&mut self, loaded: Option<usize>, library: &ModelLibrary) {
        if loaded == self.last_loaded {
            return;
        }
        self.last_loaded = loaded;
        self.scroll = true;
        if let Some(entry) = loaded.and_then(|row| library.entry(row)) {
            if !entry.folder().is_empty() {
                self.open.insert(entry.folder().to_ascii_lowercase());
            }
        }
    }
}

/// What every row reads.
struct Rows<'a> {
    library: &'a ModelLibrary,
    loaded: Option<usize>,
    filter: &'a TextFilter,
}

/// The filter field's id, fixed so a test can find the field.
pub(super) fn filter_id() -> egui::Id {
    egui::Id::unique("studio_library_filter")
}

pub(super) fn show(ui: &mut Ui, editor: &mut Editor, cx: &mut EditorContext<'_>) {
    library(ui, &mut editor.library, cx.scene, &mut editor.actions);
}

/// The filter, then the rows; a load request joins `actions`.
fn library(
    ui: &mut Ui,
    state: &mut LibraryState,
    status: SceneStatus<'_>,
    actions: &mut Vec<EditorAction>,
) {
    filter_field(ui, &mut state.filter);
    if status.library.is_empty() {
        muted_wrapped(ui, EMPTY);
        return;
    }
    state.follow(status.loaded, status.library);
    if ui.input(|input| input.pointer.any_pressed() || input.key_pressed(Key::Escape)) {
        state.armed = false;
    }
    let filter = TextFilter::parse(&state.filter);
    let rows = Rows {
        library: status.library,
        loaded: status.loaded,
        filter: &filter,
    };
    let mut load = None;
    ScrollArea::vertical()
        .auto_shrink(false)
        .id_salt("library")
        .show(ui, |ui| {
            // The outliner's tight pitch, so the highlights of adjacent rows nearly touch.
            ui.spacing_mut().item_spacing.y = (ui.spacing().item_spacing.y / 3.0).round();
            for group in status.library.groups() {
                load = group_rows(ui, state, &rows, &group).or(load);
            }
        });
    if let Some(row) = load.or_else(|| enter(ui, state, status.loaded)) {
        state.cursor = Some(row);
        actions.push(EditorAction::LoadModel(row));
    }
}

fn filter_field(ui: &mut Ui, filter: &mut String) {
    let field = TextEdit::singleline(filter)
        .id(filter_id())
        .hint_text(RichText::new("Filter").color(pal::TEXT_DISABLED))
        .margin(Margin::symmetric(8, 4))
        .min_size(vec2(0.0, frame_height(ui)))
        .desired_width(f32::INFINITY);
    ui.add(field);
}

/// One folder's rows: loose files each, a one-model folder as one row named by the folder, a larger one under a disclosure row.
fn group_rows(
    ui: &mut Ui,
    state: &mut LibraryState,
    rows: &Rows<'_>,
    group: &LibraryGroup,
) -> Option<usize> {
    let entries = rows.library.entries();
    let visible: Vec<usize> = group
        .rows
        .iter()
        .copied()
        .filter(|&row| rows.filter.passes(&entries[row].display))
        .collect();
    let first = *visible.first()?;
    if group.folder.is_empty() {
        let leaves = visible.into_iter();
        return leaves.fold(None, |load, row| {
            model_row(ui, state, rows, row, (entries[row].leaf(), "")).or(load)
        });
    }
    if group.rows.len() == 1 {
        return model_row(
            ui,
            state,
            rows,
            first,
            (&group.folder, entries[first].leaf()),
        );
    }
    let key = group.folder.to_ascii_lowercase();
    let filtering = rows.filter.is_active();
    let shown_open = filtering || state.open.contains(&key);
    if folder_row(ui, &group.folder, visible.len(), shown_open)
        && !filtering
        && !state.open.remove(&key)
    {
        state.open.insert(key.clone());
    }
    // The rows follow a click at once, as an ImGui tree node opens in the frame it is clicked.
    if !filtering && !state.open.contains(&key) {
        return None;
    }
    ui.indent(("library_folder", &group.folder), |ui| {
        visible.into_iter().fold(None, |load, row| {
            model_row(ui, state, rows, row, (entries[row].leaf(), "")).or(load)
        })
    })
    .inner
}

/// One model row with the C++ highlight, mark, Load button and tooltip; returns the row when it asks to load.
fn model_row(
    ui: &mut Ui,
    state: &mut LibraryState,
    rows: &Rows<'_>,
    row: usize,
    (name, detail): (&str, &str),
) -> Option<usize> {
    let entry = &rows.library.entries()[row];
    let loaded = rows.loaded == Some(row);
    let highlighted = state.cursor.or(rows.loaded) == Some(row);
    // A load stalls the frame, so the loaded row offers none and never reloads.
    let trailing = if highlighted && !loaded {
        RowTrailing::Action("Load")
    } else {
        RowTrailing::None
    };
    let tooltip = tooltip(entry, loaded);
    let spec = RowSpec {
        name,
        detail,
        highlighted,
        marked: loaded,
        trailing,
        tooltip: Some(&tooltip),
        ..Default::default()
    };
    let top = ui.cursor().top();
    let was_pick = state.cursor == Some(row);
    let result = ui
        .push_id(("library_row", row), |ui| list_row(ui, &spec))
        .inner;
    if loaded && state.scroll {
        let left_right = ui.max_rect().x_range();
        let rect = Rect::from_x_y_ranges(left_right, top..=top + frame_height(ui));
        ui.scroll_to_rect(rect, Some(Align::Center));
        state.scroll = false;
    }
    if result.clicked {
        state.cursor = Some(row);
        state.armed = true;
    }
    // egui pairs quick clicks on neighbouring rows into a double-click, so only the picked row loads that way.
    let double_click = result.double_clicked && was_pick;
    ((double_click || result.trailing_clicked) && !loaded).then_some(row)
}

/// The path, the scene's credit when known, and what a double-click does.
fn tooltip(entry: &LibraryEntry, loaded: bool) -> String {
    let mut text = entry.location.clone();
    if let Some(credit) = &entry.credit {
        text.push('\n');
        text.push_str(credit);
    }
    if !loaded {
        text.push_str("\nDouble-click to load");
    }
    text
}

/// A folder's row: a disclosure triangle, its name, the visible count at the right end; true when clicked.
fn folder_row(ui: &mut Ui, folder: &str, count: usize, open: bool) -> bool {
    let size = vec2(ui.available_width(), frame_height(ui));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, pal::RAISED_HOVER);
    }
    let font = TextStyle::Body.resolve(ui.style());
    let line = line_height(ui);
    let left = rect.left() + theme::FRAME_PADDING.x;
    let top = rect.top() + ((rect.height() - line) * 0.5).round();
    let triangle = pos2(left + line * 0.5, rect.center().y);
    disclosure(ui.painter(), triangle, line * 0.3, open, pal::TEXT_MUTED);
    let count = galley(ui, count.to_string(), font.clone(), pal::TEXT_MUTED);
    let count_left = rect.right() - theme::FRAME_PADDING.x - count.size().x;
    let name_left = left + line + theme::INNER_SPACING.x;
    let name_end = count_left - theme::INNER_SPACING.x;
    ellipsized(
        ui.painter(),
        pos2(name_left, top),
        name_end,
        folder,
        font,
        pal::TEXT,
    );
    ui.painter()
        .galley(pos2(count_left, top), count, pal::TEXT_MUTED);
    response.clicked()
}

/// ImGui's tree arrow: pointing right while closed, down while open.
fn disclosure(painter: &Painter, centre: Pos2, size: f32, open: bool, colour: Color32) {
    let points = if open {
        [vec2(-1.0, -0.5), vec2(1.0, -0.5), vec2(0.0, 0.6)]
    } else {
        [vec2(-0.5, -1.0), vec2(-0.5, 1.0), vec2(0.6, 0.0)]
    };
    let points = points.map(|offset| centre + offset * size).to_vec();
    painter.add(Shape::convex_polygon(points, colour, Stroke::NONE));
}

/// Enter loads the picked row while a row click armed it, as the C++ loads the row holding focus.
fn enter(ui: &Ui, state: &LibraryState, loaded: Option<usize>) -> Option<usize> {
    if !state.armed || ui.ctx().text_edit_focused() {
        return None;
    }
    let pressed = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key { key: Key::Enter, pressed: true, repeat: false, modifiers, .. }
                    if modifiers.is_none()
            )
        })
    });
    state.cursor.filter(|&row| pressed && Some(row) != loaded)
}

#[cfg(test)]
mod tests {
    use egui::{Event, Modifiers};

    use super::*;
    use crate::app::library::SceneSource;
    use crate::ui::widgets::test_support::{click, context, frame};

    /// The library alone in an 800-point frame, its rows a filter's height and a gap below the top.
    struct Shelf {
        ctx: egui::Context,
        state: LibraryState,
        library: ModelLibrary,
        loaded: Option<usize>,
    }

    impl Shelf {
        /// Rows on screen: `Dinosaurs`, the closed `GltfTest` folder, `plane` (loaded) and `suzanne`.
        fn new() -> Self {
            let entries = [
                "Dinosaurs/dinosaurs",
                "GltfTest/b",
                "GltfTest/a",
                "plane",
                "suzanne",
            ]
            .map(|display| LibraryEntry {
                display: display.to_owned(),
                location: format!("Resources/Models/{display}.gltf"),
                source: SceneSource::Url(display.to_owned()),
                credit: None,
            });
            let mut shelf = Self {
                ctx: context(),
                state: LibraryState::default(),
                library: ModelLibrary::new(entries.to_vec()),
                loaded: Some(3),
            };
            shelf.frame(Vec::new());
            shelf
        }

        fn frame(&mut self, events: Vec<Event>) -> Vec<EditorAction> {
            let mut actions = Vec::new();
            let status = SceneStatus {
                library: &self.library,
                loaded: self.loaded,
                ..SceneStatus::empty()
            };
            frame(&self.ctx, events, |ui| {
                library(ui, &mut self.state, status, &mut actions)
            });
            actions
        }

        /// The middle of the `index`-th row on screen, or of its right end where the Load button sits.
        fn row(&self, index: usize, load_button: bool) -> Vec<Event> {
            let filter = self
                .ctx
                .read_response(filter_id())
                .expect("the filter is shown")
                .rect;
            let y = filter.bottom() + 6.0 + index as f32 * 25.0 + 11.5;
            click(if load_button { 790.0 } else { 300.0 }, y)
        }

        /// Frames enough that the next click is no double-click.
        fn pause(&mut self) {
            for _ in 0..30 {
                self.frame(Vec::new());
            }
        }
    }

    fn key(key: Key) -> Vec<Event> {
        [true, false]
            .map(|pressed| Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            })
            .to_vec()
    }

    #[test]
    fn a_click_picks_and_a_double_click_loads_only_the_row_it_picked() {
        let mut shelf = Shelf::new();
        assert!(
            shelf.frame(shelf.row(3, false)).is_empty(),
            "a click only picks"
        );
        assert_eq!(shelf.state.cursor, Some(4));
        assert_eq!(
            shelf.frame(shelf.row(3, false)),
            [EditorAction::LoadModel(4)]
        );
        shelf.pause();
        shelf.frame(shelf.row(3, false));
        assert!(
            shelf.frame(shelf.row(0, false)).is_empty(),
            "a quick click on the next row picks it, it does not load it"
        );
        assert_eq!(shelf.state.cursor, Some(0));
    }

    #[test]
    fn enter_loads_the_pick_until_another_press_or_esc_disarms_it() {
        let mut shelf = Shelf::new();
        shelf.frame(shelf.row(0, false));
        assert_eq!(shelf.frame(key(Key::Enter)), [EditorAction::LoadModel(0)]);
        shelf.pause();
        shelf.frame(shelf.row(0, false));
        shelf.frame(click(300.0, 500.0));
        assert!(
            shelf.frame(key(Key::Enter)).is_empty(),
            "a press elsewhere disarms"
        );
        shelf.pause();
        shelf.frame(shelf.row(0, false));
        shelf.frame(key(Key::Escape));
        assert!(shelf.frame(key(Key::Enter)).is_empty(), "Esc disarms");
    }

    #[test]
    fn the_loaded_row_never_reloads_and_the_pick_has_a_load_button() {
        let mut shelf = Shelf::new();
        shelf.frame(shelf.row(2, false));
        assert!(
            shelf.frame(shelf.row(2, false)).is_empty(),
            "no double-click reload"
        );
        assert!(shelf.frame(key(Key::Enter)).is_empty(), "no Enter reload");
        shelf.pause();
        shelf.frame(shelf.row(3, false));
        // The button appears with the pick, and egui hits what the frame before drew.
        shelf.pause();
        assert_eq!(
            shelf.frame(shelf.row(3, true)),
            [EditorAction::LoadModel(4)]
        );
    }

    #[test]
    fn a_folder_opens_on_a_click_or_while_filtering_and_holds_its_loaded_row_open() {
        let mut shelf = Shelf::new();
        shelf.frame(shelf.row(1, false));
        shelf.pause();
        shelf.frame(shelf.row(2, false));
        assert_eq!(
            shelf.state.cursor,
            Some(2),
            "the open folder lists `a` before `b`"
        );
        shelf.pause();
        shelf.frame(shelf.row(1, false));
        shelf.pause();
        shelf.frame(shelf.row(2, false));
        assert_eq!(
            shelf.state.cursor,
            Some(3),
            "closed again, `plane` follows the folder"
        );
        shelf.state.filter = "-a, gltf".to_owned();
        shelf.pause();
        shelf.frame(shelf.row(1, false));
        assert_eq!(
            shelf.state.cursor,
            Some(1),
            "filtering opens the folder, which shows `b` alone"
        );
        let mut loaded_in_folder = Shelf::new();
        loaded_in_folder.loaded = Some(1);
        loaded_in_folder.frame(Vec::new());
        assert!(loaded_in_folder.state.open.contains("gltftest"));
    }

    #[test]
    fn a_tooltip_names_the_path_the_credit_and_the_double_click() {
        let entry = LibraryEntry {
            display: "Colosseum-gltf/Colosseum-gltf".into(),
            location: "resources/scenes/Colosseum-gltf/Colosseum-gltf.gltf".into(),
            source: SceneSource::Url(String::new()),
            credit: Some("Colosseum by Sulo456, CC BY 4.0".into()),
        };
        assert_eq!(
            tooltip(&entry, false),
            "resources/scenes/Colosseum-gltf/Colosseum-gltf.gltf\nColosseum by Sulo456, CC BY 4.0\nDouble-click to load"
        );
        assert!(!tooltip(&entry, true).contains("Double-click"));
    }
}
