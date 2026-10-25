//! New-session popup: one line of text.

use tui_realm_stdlib::components::Input;
use tuirealm::command::{Cmd, Direction, Position};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{HorizontalAlignment, Style, Title};
use tuirealm::ratatui::style::Color;
use tuirealm::state::{State, StateValue};

use super::{borders, key};
use crate::app::Msg;

#[derive(Component)]
pub struct NameInput {
    component: Input,
}

impl Default for NameInput {
    fn default() -> Self {
        Self {
            component: Input::default()
                .borders(borders(true))
                .title(Title::from(" New session ").alignment(HorizontalAlignment::Left))
                .placeholder(tuirealm::ratatui::text::Line::styled(
                    "Name, then Enter",
                    Style::new().fg(Color::DarkGray),
                )),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for NameInput {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        let cmd = match key(ev)? {
            (Key::Esc, _) => return Some(Msg::ClosePopup),
            (Key::Enter, _) => {
                return match self.component.state() {
                    State::Single(StateValue::String(name)) => Some(Msg::NewSession(name)),
                    _ => Some(Msg::NewSession(String::new())),
                };
            }
            (Key::Char(c), false) => Cmd::Type(c),
            (Key::Backspace, _) => Cmd::Delete,
            (Key::Delete, _) => Cmd::Cancel,
            (Key::Left, _) => Cmd::Move(Direction::Left),
            (Key::Right, _) => Cmd::Move(Direction::Right),
            (Key::Home, _) | (Key::Char('a'), true) => Cmd::GoTo(Position::Begin),
            (Key::End, _) | (Key::Char('e'), true) => Cmd::GoTo(Position::End),
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

    #[test]
    fn typing_then_enter_names_the_session() {
        let mut input = NameInput::default();
        for c in "Perfx".chars() {
            input.on(&Event::Keyboard(KeyEvent::new(
                Key::Char(c),
                KeyModifiers::NONE,
            )));
        }
        input.on(&Event::Keyboard(KeyEvent::new(
            Key::Backspace,
            KeyModifiers::NONE,
        )));
        let enter = Event::Keyboard(KeyEvent::new(Key::Enter, KeyModifiers::NONE));
        assert_eq!(input.on(&enter), Some(Msg::NewSession("Perf".into())));
        let esc = Event::Keyboard(KeyEvent::new(Key::Esc, KeyModifiers::NONE));
        assert_eq!(input.on(&esc), Some(Msg::ClosePopup));
    }
}
