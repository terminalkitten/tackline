//! Ctrl+O: pick the active session.

use tui_realm_stdlib::components::List;
use tuirealm::command::{Cmd, Direction};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{AttrValue, HorizontalAlignment, PropPayload, PropValue, Style, Title};
use tuirealm::ratatui::style::{Color, Modifier};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::state::{State, StateValue};

use super::{borders, key};
use crate::app::Msg;
use crate::model::{Session, note_count};

/// Rows for the list: name and note count, `●` on the active one.
pub fn rows(sessions: &[Session], active: Option<&str>) -> AttrValue {
    let rows = sessions.iter().map(|s| {
        let mark = if Some(s.id.as_str()) == active {
            "● "
        } else {
            "  "
        };
        Line::from(vec![
            Span::styled(mark, Style::new().fg(Color::Cyan)),
            Span::raw(s.name.clone()),
            Span::styled(
                format!("  {}", note_count(s.notes.len())),
                Style::new().fg(Color::DarkGray),
            ),
        ])
    });
    AttrValue::Payload(PropPayload::Vec(rows.map(PropValue::TextLine).collect()))
}

pub fn selected(index: usize) -> AttrValue {
    AttrValue::Payload(PropPayload::Single(PropValue::Usize(index)))
}

#[derive(Component)]
pub struct SessionPicker {
    component: List,
}

impl Default for SessionPicker {
    fn default() -> Self {
        Self {
            component: List::default()
                .borders(borders(true))
                .title(Title::from(" Sessions ").alignment(HorizontalAlignment::Left))
                .scroll(true)
                .rewind(true)
                .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for SessionPicker {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        let cmd = match key(ev)? {
            (Key::Esc | Key::Char('q'), _) => return Some(Msg::ClosePopup),
            (Key::Enter, _) => {
                return match self.component.state() {
                    State::Single(StateValue::Usize(i)) => Some(Msg::PickSession(i)),
                    _ => None,
                };
            }
            (Key::Down | Key::Char('j'), false) => Cmd::Move(Direction::Down),
            (Key::Up | Key::Char('k'), false) => Cmd::Move(Direction::Up),
            _ => return None,
        };
        self.component.perform(cmd);
        Some(Msg::Redraw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::event::{KeyEvent, KeyModifiers};
    use tuirealm::props::Attribute;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    #[test]
    fn pick_with_keys() {
        let sessions: Vec<_> = ["Perf", "Docs", "Bugs"].map(Session::new).into();
        let mut picker = SessionPicker::default();
        picker.attr(Attribute::Text, rows(&sessions, Some(&sessions[1].id)));
        picker.attr(Attribute::Value, selected(1));
        insta::assert_snapshot!(render_to_string(&mut picker, Size::new(30, 5)));
        let press =
            |p: &mut SessionPicker, k| p.on(&Event::Keyboard(KeyEvent::new(k, KeyModifiers::NONE)));
        press(&mut picker, Key::Char('j'));
        assert_eq!(press(&mut picker, Key::Enter), Some(Msg::PickSession(2)));
        press(&mut picker, Key::Char('j'));
        assert_eq!(
            press(&mut picker, Key::Enter),
            Some(Msg::PickSession(0)),
            "wraps around"
        );
    }
}
