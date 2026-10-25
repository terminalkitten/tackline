//! Short message in the bottom-right corner.

use tui_realm_stdlib::components::Paragraph;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, NoUserEvent};
use tuirealm::props::{BorderType, Borders, Color};
use tuirealm::ratatui::layout::Rect;

use crate::app::{Msg, ToastKind};

pub fn borders(kind: ToastKind) -> Borders {
    let color = match kind {
        ToastKind::Info => Color::Green,
        ToastKind::Warning => Color::Yellow,
        ToastKind::Error => Color::Red,
    };
    Borders::default()
        .modifiers(BorderType::Rounded)
        .color(color)
}

/// Bottom-right inside `pane`'s border.
pub fn area(pane: Rect, text: &str) -> Rect {
    let screen = pane.inner(tuirealm::ratatui::layout::Margin::new(1, 0));
    let width = (text.chars().count() as u16 + 4).min(screen.width);
    let x = screen.right().saturating_sub(width);
    let y = screen.bottom().saturating_sub(4);
    Rect::new(x, y, width, 3).intersection(screen)
}

#[derive(Component, Default)]
pub struct Toast {
    component: Paragraph,
}

impl AppComponent<Msg, NoUserEvent> for Toast {
    fn on(&mut self, _: &Event<NoUserEvent>) -> Option<Msg> {
        None
    }
}
