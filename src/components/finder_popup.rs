//! Query input shared by Ctrl+P, Ctrl+F and `/`, plus the results list.
//! The model decides what a change or Enter means for each popup.

use tui_realm_stdlib::components::Input;
use tuirealm::command::{Cmd, Direction, Position};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{HorizontalAlignment, Title};
use tuirealm::ratatui::layout::{Constraint, Layout, Rect};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::state::{State, StateValue};

use super::code_view::{CodeProps, Rows};
use super::{borders, centered, key};
use crate::app::{Msg, Picker, PickerKind};

/// Input and results areas of the Ctrl+P / Ctrl+F popup.
pub fn areas(screen: Rect) -> (Rect, Rect) {
    let area = centered(
        screen,
        100.min(screen.width.saturating_sub(4)),
        24.min(screen.height.saturating_sub(2)),
    );
    let [input, results] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(area);
    (input, results)
}

/// The `/` input: a bar at the bottom of the code pane.
pub fn find_area(code: Rect) -> Rect {
    Rect::new(
        code.x,
        code.bottom().saturating_sub(3),
        code.width,
        3.min(code.height),
    )
}

pub fn title(kind: Option<PickerKind>) -> &'static str {
    match kind {
        Some(PickerKind::Files) => " Find file ",
        Some(PickerKind::Code) => " Search code (Enter searches, Enter again opens) ",
        None => " Find in file (n / N: next / previous) ",
    }
}

pub fn results(picker: &Picker, height: usize) -> CodeProps {
    let dim = Style::new().fg(Color::DarkGray);
    let rows = picker
        .hits
        .iter()
        .enumerate()
        .map(|(i, hit)| {
            let line = match picker.kind {
                PickerKind::Files => Line::from(hit.path.display().to_string()),
                PickerKind::Code => Line::from(vec![
                    Span::styled(format!("{}:{}: ", hit.path.display(), hit.line), dim),
                    Span::raw(hit.text.clone()),
                ]),
            };
            if i == picker.selected {
                line.style(Style::new().add_modifier(Modifier::REVERSED))
            } else {
                line
            }
        })
        .collect();
    let count = match (picker.kind, picker.hits.len()) {
        (PickerKind::Code, 0) => String::new(),
        (_, n) => n.to_string(),
    };
    CodeProps {
        title: count,
        rows,
        focused: true,
        scroll: picker.selected.saturating_sub(height.saturating_sub(1)) as u16,
        footer: None,
    }
}

#[derive(Component)]
pub struct QueryInput {
    component: Input,
}

impl Default for QueryInput {
    fn default() -> Self {
        Self {
            component: Input::default().borders(borders(true)),
        }
    }
}

impl QueryInput {
    fn value(&self) -> String {
        match self.component.state() {
            State::Single(StateValue::String(s)) => s,
            _ => String::new(),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for QueryInput {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        let cmd = match key(ev)? {
            (Key::Esc, _) => return Some(Msg::ClosePopup),
            (Key::Enter, _) => return Some(Msg::QueryEnter(self.value())),
            (Key::Down, _) | (Key::Char('n'), true) => return Some(Msg::PickerMove(1)),
            (Key::Up, _) | (Key::Char('p'), true) => return Some(Msg::PickerMove(-1)),
            (Key::PageDown, _) => return Some(Msg::PickerMove(10)),
            (Key::PageUp, _) => return Some(Msg::PickerMove(-10)),
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
        Some(Msg::QueryChanged(self.value()))
    }
}

/// Title for the input, set when a popup opens.
pub fn title_attr(kind: Option<PickerKind>) -> Title {
    Title::from(title(kind)).alignment(HorizontalAlignment::Left)
}

#[derive(Component, Default)]
pub struct PickerResults {
    component: Rows,
}

impl AppComponent<Msg, NoUserEvent> for PickerResults {
    fn on(&mut self, _: &Event<NoUserEvent>) -> Option<Msg> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::event::{KeyEvent, KeyModifiers};

    #[test]
    fn typing_reports_the_query() {
        let mut input = QueryInput::default();
        let press = |i: &mut QueryInput, k, m| i.on(&Event::Keyboard(KeyEvent::new(k, m)));
        press(&mut input, Key::Char('a'), KeyModifiers::NONE);
        assert_eq!(
            press(&mut input, Key::Char('b'), KeyModifiers::NONE),
            Some(Msg::QueryChanged("ab".into()))
        );
        assert_eq!(
            press(&mut input, Key::Backspace, KeyModifiers::NONE),
            Some(Msg::QueryChanged("a".into()))
        );
        assert_eq!(
            press(&mut input, Key::Down, KeyModifiers::NONE),
            Some(Msg::PickerMove(1))
        );
        assert_eq!(
            press(&mut input, Key::Enter, KeyModifiers::NONE),
            Some(Msg::QueryEnter("a".into()))
        );
    }
}
