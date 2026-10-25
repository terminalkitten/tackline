//! Yes/no question; the model knows what it's about.

use tui_realm_stdlib::components::Paragraph;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{AttrValue, Attribute, Color, HorizontalAlignment, Title};
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::text::Text;

use super::{borders, centered, key};
use crate::app::{Msg, Popup};

/// Title and hint for each question.
pub fn question(popup: Popup) -> (&'static str, &'static str) {
    match popup {
        Popup::Gitignore => (
            " Add .tackline/ to .gitignore? ",
            " y / Enter: add it    n / Esc: no, don't ask again ",
        ),
        _ => (" Delete note? ", " y / Enter: delete    n / Esc: keep "),
    }
}

pub fn attrs(popup: Popup) -> [(Attribute, AttrValue); 2] {
    let (title, hint) = question(popup);
    [
        (
            Attribute::Title,
            AttrValue::Title(Title::from(title).alignment(HorizontalAlignment::Left)),
        ),
        (Attribute::Text, AttrValue::Text(Text::from(hint))),
    ]
}

pub fn area(screen: Rect, popup: Popup) -> Rect {
    let (title, hint) = question(popup);
    let width = title.chars().count().max(hint.chars().count()) as u16 + 4;
    centered(screen, width.min(screen.width), 3)
}

#[derive(Component)]
pub struct Confirm {
    component: Paragraph,
}

impl Default for Confirm {
    fn default() -> Self {
        Self {
            component: Paragraph::default().borders(borders(true).color(Color::Yellow)),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for Confirm {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match key(ev)? {
            (Key::Char('y') | Key::Enter, false) => Some(Msg::Confirm(true)),
            (Key::Char('n') | Key::Esc | Key::Char('q'), false) => Some(Msg::Confirm(false)),
            _ => None,
        }
    }
}
