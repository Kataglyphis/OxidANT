//! The editor's keys: one table feeds both the shortcut handlers and the Camera tab's CONTROLS list.

use egui::{Context, InputState, Key};

use super::EditorAction;
use crate::app::HostCaps;

/// What a bound key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    ToggleEditor,
    ReloadShaders,
    Screenshot,
    Quit,
}

/// One row of the CONTROLS list; mouse rows carry no shortcut.
#[derive(Clone, Copy, Debug)]
pub struct KeyBinding {
    pub keys: &'static [&'static str],
    pub action: &'static str,
    pub shortcut: Option<(Key, Shortcut)>,
}

pub const KEY_BINDINGS: &[KeyBinding] = &[
    KeyBinding {
        keys: &["Left drag"],
        action: "Orbit around the target",
        shortcut: None,
    },
    KeyBinding {
        keys: &["Wheel"],
        action: "Zoom in and out",
        shortcut: None,
    },
    KeyBinding {
        keys: &["ESC"],
        action: "Leave the focused field or menu; pressed again, quit",
        shortcut: Some((Key::Escape, Shortcut::Quit)),
    },
    KeyBinding {
        keys: &["F1"],
        action: "Show or hide the editor",
        shortcut: Some((Key::F1, Shortcut::ToggleEditor)),
    },
    KeyBinding {
        keys: &["F5"],
        action: "Reload all shaders",
        shortcut: Some((Key::F5, Shortcut::ReloadShaders)),
    },
    KeyBinding {
        keys: &["S"],
        action: "Save a screenshot",
        shortcut: Some((Key::S, Shortcut::Screenshot)),
    },
];

impl KeyBinding {
    /// What the row says on this host; `None` hides a key the host cannot act on.
    pub fn action_on(&self, caps: HostCaps) -> Option<&'static str> {
        match self.shortcut.map(|(_, shortcut)| shortcut) {
            Some(Shortcut::Quit) if !caps.quit => Some("Leave the focused field or menu"),
            Some(Shortcut::ReloadShaders) if !caps.hot_reload => None,
            Some(Shortcut::Screenshot) if !caps.screenshot => None,
            _ => Some(self.action),
        }
    }
}

/// The table's keys pressed this pass; none while typing, so a field can take an `s`.
pub(super) fn shortcuts(
    ctx: &Context,
    esc_armed: bool,
    caps: HostCaps,
    visible: &mut bool,
) -> Vec<EditorAction> {
    if ctx.text_edit_focused() {
        return Vec::new();
    }
    let pressed: Vec<Shortcut> = ctx.input(|input| {
        KEY_BINDINGS
            .iter()
            .filter_map(|binding| binding.shortcut)
            .filter(|&(key, _)| pressed_once(input, key))
            .map(|(_, shortcut)| shortcut)
            .collect()
    });
    let mut actions = Vec::new();
    for shortcut in pressed {
        match shortcut {
            Shortcut::ToggleEditor => *visible = !*visible,
            Shortcut::ReloadShaders if caps.hot_reload => actions.push(EditorAction::ReloadShaders),
            Shortcut::Screenshot if caps.screenshot => actions.push(EditorAction::Screenshot),
            Shortcut::Quit if caps.quit && esc_armed => actions.push(EditorAction::Quit),
            _ => {}
        }
    }
    actions
}

/// A first press of `key` without modifiers; key repeats do not count.
fn pressed_once(input: &InputState, key: Key) -> bool {
    input.events.iter().any(|event| {
        matches!(
            event,
            egui::Event::Key { key: event_key, pressed: true, repeat: false, modifiers, .. }
                if *event_key == key && modifiers.is_none()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_handled_key_is_listed_under_its_own_name() {
        let names = |key: Key| match key {
            Key::Escape => "ESC".to_owned(),
            other => other.name().to_owned(),
        };
        for binding in KEY_BINDINGS {
            if let Some((key, _)) = binding.shortcut {
                assert_eq!(binding.keys, [names(key).as_str()], "{binding:?}");
            }
        }
        let handled: Vec<Shortcut> = KEY_BINDINGS
            .iter()
            .filter_map(|b| b.shortcut)
            .map(|(_, s)| s)
            .collect();
        for shortcut in [
            Shortcut::ToggleEditor,
            Shortcut::ReloadShaders,
            Shortcut::Screenshot,
            Shortcut::Quit,
        ] {
            assert!(handled.contains(&shortcut), "{shortcut:?} has no row");
        }
    }

    #[test]
    fn the_browser_lists_no_file_keys_and_an_esc_that_never_quits() {
        let browser = HostCaps::default();
        let rows: Vec<&str> = KEY_BINDINGS
            .iter()
            .filter_map(|b| b.action_on(browser))
            .collect();
        assert_eq!(rows.len(), 4);
        assert!(rows.contains(&"Leave the focused field or menu"));
        assert!(!rows
            .iter()
            .any(|row| row.contains("quit") || row.contains("shaders")));
    }
}
