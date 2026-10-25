//! Thin components: they turn key events into `Msg`s and render from props.

pub mod code_view;
pub mod file_tree;
pub mod help_popup;
pub mod status_bar;

use tui_realm_stdlib::components::Paragraph;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyModifiers, NoUserEvent};
use tuirealm::props::{BorderType, Borders, Color};

use crate::app::Msg;
use crate::model::View;

/// The key and whether Ctrl is held. Shift is already in the char (`G`, `?`).
pub fn key(ev: &Event<NoUserEvent>) -> Option<(Key, bool)> {
    let k = ev.as_keyboard()?;
    Some((k.code, k.modifiers.contains(KeyModifiers::CONTROL)))
}

/// Keys that work in every pane when no popup is open.
pub fn global_key(ev: &Event<NoUserEvent>) -> Option<Msg> {
    match key(ev)? {
        (Key::Char('1'), false) => Some(Msg::ShowView(View::Files)),
        (Key::Char('2'), false) => Some(Msg::ShowView(View::Diff)),
        (Key::Char('3'), false) => Some(Msg::ShowView(View::Session)),
        (Key::Char('?'), false) => Some(Msg::ToggleHelp),
        (Key::Char('q'), false) => Some(Msg::Quit),
        _ => None,
    }
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

/// Stand-in for views that aren't built yet.
#[derive(Component)]
pub struct Placeholder {
    component: Paragraph,
}

impl Default for Placeholder {
    fn default() -> Self {
        Self {
            component: Paragraph::default().borders(borders(true)),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for Placeholder {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        global_key(ev)
    }
}
