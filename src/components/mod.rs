//! Thin components: they turn key events into `Msg`s and render from props.

pub mod code_view;
pub mod confirm;
pub mod diff_view;
pub mod file_tree;
pub mod finder_popup;
pub mod help_popup;
pub mod name_input;
pub mod note_popup;
pub mod session_picker;
pub mod session_view;
pub mod status_bar;
pub mod toast;

use tuirealm::event::{Event, Key, KeyModifiers, MouseButton, MouseEventKind, NoUserEvent};
use tuirealm::props::{BorderType, Borders, Color};
use tuirealm::ratatui::layout::{Constraint, Flex, Layout, Rect};

use crate::app::{Msg, Popup};
use crate::model::View;

/// The key and whether Ctrl is held. Shift is already in the char (`G`, `?`).
pub fn key(ev: &Event<NoUserEvent>) -> Option<(Key, bool)> {
    let k = ev.as_keyboard()?;
    Some((k.code, k.modifiers.contains(KeyModifiers::CONTROL)))
}

/// Wheel, click and drag; the model routes them by screen position.
pub fn mouse(ev: &Event<NoUserEvent>) -> Option<Msg> {
    let Event::Mouse(m) = ev else { return None };
    let (col, row) = (m.column, m.row);
    match m.kind {
        MouseEventKind::ScrollDown => Some(Msg::Wheel {
            col,
            row,
            lines: WHEEL_LINES,
        }),
        MouseEventKind::ScrollUp => Some(Msg::Wheel {
            col,
            row,
            lines: -WHEEL_LINES,
        }),
        MouseEventKind::Down(MouseButton::Left) => Some(Msg::Click { col, row }),
        MouseEventKind::Drag(MouseButton::Left) => Some(Msg::Drag { col, row }),
        _ => None,
    }
}

/// Lines per wheel step.
// ponytail: fixed; make it a setting if trackpads feel too fast or slow.
const WHEEL_LINES: isize = 3;

/// Keys that work in every pane when no popup is open.
pub fn global_key(ev: &Event<NoUserEvent>) -> Option<Msg> {
    match key(ev)? {
        (Key::Char('1'), false) => Some(Msg::ShowView(View::Files)),
        (Key::Char('2'), false) => Some(Msg::ShowView(View::Diff)),
        (Key::Char('3'), false) => Some(Msg::ShowView(View::Session)),
        (Key::Char('?'), false) => Some(Msg::OpenPopup(Popup::Help)),
        (Key::Char('n'), true) => Some(Msg::OpenPopup(Popup::NewSession)),
        (Key::Char('o'), true) => Some(Msg::OpenPopup(Popup::SessionPicker)),
        (Key::Char('p'), true) => Some(Msg::OpenPopup(Popup::Finder)),
        (Key::Char('f'), true) => Some(Msg::OpenPopup(Popup::Search)),
        (Key::Char('q'), false) => Some(Msg::Quit),
        _ => None,
    }
}

/// A `width` × `height` box in the middle of `screen`.
pub fn centered(screen: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(screen);
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    area
}

pub fn borders(focused: bool) -> Borders {
    Borders::default()
        .modifiers(BorderType::Rounded)
        .color(if focused {
            Color::Cyan
        } else {
            Color::DarkGray
        })
}
