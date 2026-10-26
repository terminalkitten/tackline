//! New-session and rename-session popup: one line of text.

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
    /// Renaming the active session instead of creating one.
    rename: bool,
}

impl Default for NameInput {
    fn default() -> Self {
        Self::new(None)
    }
}

impl NameInput {
    /// A new session, or with `rename` the active session's current name to edit.
    pub fn new(rename: Option<&str>) -> Self {
        let title = if rename.is_some() {
            " Rename session "
        } else {
            " New session "
        };
        let mut component = Input::default()
            .borders(borders(true))
            .title(Title::from(title).alignment(HorizontalAlignment::Left))
            .placeholder(tuirealm::ratatui::text::Line::styled(
                "Name, then Enter",
                Style::new().fg(Color::DarkGray),
            ));
        if let Some(name) = rename {
            component = component.value(name);
        }
        Self {
            component,
            rename: rename.is_some(),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for NameInput {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        let cmd = match key(ev)? {
            (Key::Esc, _) => return Some(Msg::ClosePopup),
            (Key::Enter, _) => {
                let name = match self.component.state() {
                    State::Single(StateValue::String(name)) => name,
                    _ => String::new(),
                };
                return Some(if self.rename {
                    Msg::RenameSession(name)
                } else {
                    Msg::NewSession(name)
                });
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

    #[test]
    fn rename_starts_with_the_current_name() {
        let mut input = NameInput::new(Some("Perf"));
        input.on(&Event::Keyboard(KeyEvent::new(
            Key::Char('!'),
            KeyModifiers::NONE,
        )));
        let enter = Event::Keyboard(KeyEvent::new(Key::Enter, KeyModifiers::NONE));
        assert_eq!(input.on(&enter), Some(Msg::RenameSession("Perf!".into())));
    }
}
