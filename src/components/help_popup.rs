use tui_realm_stdlib::components::Paragraph;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{HorizontalAlignment, Title};
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::text::{Line, Text};

use super::{borders, centered, key};
use crate::app::Msg;

/// Keys that work today; grows with each milestone.
const KEYS: &[(&str, &str)] = &[
    ("1 / 2 / 3", "Files / Diff / Session view"),
    ("Ctrl+N", "New session"),
    ("Ctrl+O", "Pick the active session"),
    ("Tab", "Switch tree ↔ code"),
    ("j k ↑ ↓", "Move down / up"),
    ("h l ← →", "Tree: close / open · Code: column"),
    ("Enter", "Open file / toggle directory"),
    ("gg / G", "Top / bottom"),
    ("Ctrl+D / Ctrl+U", "Half page down / up"),
    ("0 / $", "Line start / end"),
    ("v / V", "Select chars / lines (again: stop)"),
    ("Shift+arrows", "Select lines (↑↓) or chars (←→)"),
    ("c", "Note the selection or line"),
    ("Esc", "Clear the selection"),
    ("Session: j k", "Select note"),
    ("Session: Enter", "Jump to the note"),
    ("Session: y", "Copy the compiled prompt"),
    ("Session: e / d", "Edit / delete the note"),
    ("Session: J / K", "Move the note down / up"),
    ("?", "This help"),
    ("q", "Quit"),
];

pub fn area(screen: Rect) -> Rect {
    // " {key:<16}{desc}" plus a space and the two borders.
    let width = KEYS
        .iter()
        .map(|(_, d)| d.chars().count() + 20)
        .max()
        .unwrap_or(40) as u16;
    centered(screen, width, (KEYS.len() as u16 + 2).min(screen.height))
}

#[derive(Component)]
pub struct HelpPopup {
    component: Paragraph,
}

impl Default for HelpPopup {
    fn default() -> Self {
        let lines: Vec<Line> = KEYS
            .iter()
            .map(|(k, d)| Line::from(format!(" {k:<16}{d}")))
            .collect();
        Self {
            component: Paragraph::default()
                .borders(borders(true))
                .title(Title::from(" Keys ").alignment(HorizontalAlignment::Center))
                .text(Text::from(lines)),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for HelpPopup {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match key(ev)? {
            (Key::Esc | Key::Char('?') | Key::Char('q'), false) => Some(Msg::ClosePopup),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    #[test]
    fn renders() {
        let size = area(Rect::new(0, 0, 100, 40));
        insta::assert_snapshot!(render_to_string(
            &mut HelpPopup::default(),
            Size::new(size.width, size.height)
        ));
    }
}
