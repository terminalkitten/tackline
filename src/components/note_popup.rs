//! Note popup: a header the model fills (location, code, target session) and
//! a text box (tui-realm-textarea). The text is widget state; it's remounted
//! with the starting text each time the popup opens.

use tui_realm_stdlib::components::Paragraph;
use tui_realm_textarea::{TEXTAREA_CMD_NEWLINE, TextArea};
use tuirealm::command::{Cmd, Direction, Position};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyModifiers, NoUserEvent};
use tuirealm::props::{HorizontalAlignment, Title};
use tuirealm::ratatui::layout::{Constraint, Layout, Rect};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span, Text};
use tuirealm::state::{State, StateValue};

use super::code_view::{CodeProps, Rows};
use super::{borders, centered, key};
use crate::app::Msg;

/// Code lines shown in the header.
const CODE_LINES: usize = 4;
const TEXT_HEIGHT: u16 = 7;

/// Header rows: location, first lines of the code, target session.
pub fn header(
    title: &str,
    location: &str,
    code: &str,
    session: &str,
    can_cycle: bool,
) -> CodeProps {
    let dim = Style::new().fg(Color::DarkGray);
    let mut rows = vec![Line::styled(
        location.to_string(),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    let lines: Vec<&str> = code.lines().collect();
    for line in lines.iter().take(CODE_LINES) {
        rows.push(Line::from(vec![
            Span::styled("│ ", dim),
            Span::raw(line.to_string()),
        ]));
    }
    if lines.len() > CODE_LINES {
        rows.push(Line::styled(
            format!("│ … {} more lines", lines.len() - CODE_LINES),
            dim,
        ));
    }
    rows.push(Line::default());
    let session = if can_cycle {
        Line::from(vec![
            Span::raw("Session: "),
            Span::styled(format!("◀ {session} ▶"), Style::new().fg(Color::Cyan)),
            Span::styled("  Tab / Shift+Tab", dim),
        ])
    } else {
        Line::from(format!("Session: {session}"))
    };
    rows.push(session);
    CodeProps {
        title: title.to_string(),
        rows,
        focused: true,
        scroll: 0,
    }
}

/// Header and text box areas.
pub fn areas(screen: Rect, header_rows: usize) -> (Rect, Rect) {
    let height = header_rows as u16 + 2 + TEXT_HEIGHT;
    let area = centered(screen, 80.min(screen.width), height.min(screen.height));
    let [head, text] = Layout::vertical([
        Constraint::Length(header_rows as u16 + 2),
        Constraint::Min(3),
    ])
    .areas(area);
    (head, text)
}

#[derive(Component, Default)]
pub struct NoteHeader {
    component: Rows,
}

impl AppComponent<Msg, NoUserEvent> for NoteHeader {
    fn on(&mut self, _: &Event<NoUserEvent>) -> Option<Msg> {
        None
    }
}

#[derive(Component)]
pub struct NoteText {
    component: TextArea<'static>,
}

impl NoteText {
    pub fn new(text: &str) -> Self {
        let lines = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(str::to_string).collect()
        };
        let mut component = TextArea::new(lines)
            .borders(borders(true))
            .title(
                Title::from(" Prompt · Enter saves · Shift+Enter newline · Esc cancels ")
                    .alignment(HorizontalAlignment::Left),
            )
            .cursor_style(Style::new().add_modifier(Modifier::REVERSED))
            // tui-textarea underlines the cursor line by default.
            .cursor_line_style(Style::new())
            .layout_margin(0);
        // Start at the end of any existing text.
        component.perform(Cmd::Custom(tui_realm_textarea::TEXTAREA_CMD_MOVE_BOTTOM));
        component.perform(Cmd::GoTo(Position::End));
        Self { component }
    }

    fn text(&self) -> String {
        match self.component.state() {
            State::Vec(lines) => lines
                .into_iter()
                .filter_map(|l| match l {
                    StateValue::String(s) => Some(s),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for NoteText {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        let k = ev.as_keyboard()?;
        // Shift+Enter needs a terminal that reports it (kitty protocol);
        // Alt+Enter and Ctrl+J work everywhere.
        let newline = k.code == Key::Enter
            && k.modifiers
                .intersects(KeyModifiers::SHIFT | KeyModifiers::ALT);
        let cmd = match key(ev)? {
            _ if newline => Cmd::Custom(TEXTAREA_CMD_NEWLINE),
            (Key::Esc, _) => return Some(Msg::ClosePopup),
            (Key::Enter, false) | (Key::Char('s'), true) => {
                return Some(Msg::SaveNote(self.text()));
            }
            (Key::Char('j'), true) => Cmd::Custom(TEXTAREA_CMD_NEWLINE),
            (Key::Tab, _) => return Some(Msg::CycleSession(1)),
            (Key::BackTab, _) => return Some(Msg::CycleSession(-1)),
            (Key::Char(c), false) => Cmd::Type(c),
            (Key::Backspace, _) => Cmd::Delete,
            (Key::Delete, _) => Cmd::Cancel,
            (Key::Left, _) => Cmd::Move(Direction::Left),
            (Key::Right, _) => Cmd::Move(Direction::Right),
            (Key::Up, _) => Cmd::Move(Direction::Up),
            (Key::Down, _) => Cmd::Move(Direction::Down),
            (Key::Home, _) | (Key::Char('a'), true) => Cmd::GoTo(Position::Begin),
            (Key::End, _) | (Key::Char('e'), true) => Cmd::GoTo(Position::End),
            _ => return None,
        };
        self.component.perform(cmd);
        Some(Msg::Redraw)
    }
}

/// `d` in the Session view asks first.
#[derive(Component)]
pub struct ConfirmDelete {
    component: Paragraph,
}

impl Default for ConfirmDelete {
    fn default() -> Self {
        Self {
            component: Paragraph::default()
                .borders(borders(true).color(Color::Yellow))
                .title(Title::from(" Delete note? ").alignment(HorizontalAlignment::Left))
                .text(Text::from(" y / Enter: delete    n / Esc: keep")),
        }
    }
}

impl AppComponent<Msg, NoUserEvent> for ConfirmDelete {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match key(ev)? {
            (Key::Char('y') | Key::Enter, false) => Some(Msg::DeleteNote),
            (Key::Char('n') | Key::Esc | Key::Char('q'), false) => Some(Msg::ClosePopup),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::event::{KeyEvent, KeyModifiers};
    use tuirealm::props::{AttrValue, Attribute, PropBound, PropPayload};
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    fn press(
        c: &mut impl AppComponent<Msg, NoUserEvent>,
        code: Key,
        mods: KeyModifiers,
    ) -> Option<Msg> {
        c.on(&Event::Keyboard(KeyEvent::new(code, mods)))
    }

    #[test]
    fn enter_saves_shift_enter_is_newline() {
        let mut text = NoteText::new("");
        for c in "make it async".chars() {
            press(&mut text, Key::Char(c), KeyModifiers::NONE);
        }
        press(&mut text, Key::Enter, KeyModifiers::SHIFT);
        for c in "keep the API".chars() {
            press(&mut text, Key::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(
            press(&mut text, Key::Tab, KeyModifiers::NONE),
            Some(Msg::CycleSession(1))
        );
        press(&mut text, Key::Enter, KeyModifiers::ALT);
        press(&mut text, Key::Char('j'), KeyModifiers::CONTROL);
        press(&mut text, Key::Char('!'), KeyModifiers::NONE);
        assert_eq!(
            press(&mut text, Key::Enter, KeyModifiers::NONE),
            Some(Msg::SaveNote("make it async\nkeep the API\n\n!".into()))
        );
    }

    #[test]
    fn edit_starts_with_text() {
        let mut text = NoteText::new("old\ntext");
        press(&mut text, Key::Char('!'), KeyModifiers::NONE);
        assert_eq!(
            press(&mut text, Key::Char('s'), KeyModifiers::CONTROL),
            Some(Msg::SaveNote("old\ntext!".into()))
        );
    }

    #[test]
    fn header_renders() {
        let props = header(
            "Add note",
            "src/matcher.rs:L3-7",
            "a\nb\nc\nd\ne\nf",
            "Perf",
            true,
        );
        let mut head = NoteHeader::default();
        head.attr(
            Attribute::Value,
            AttrValue::Payload(PropPayload::Any(props.to_any_prop())),
        );
        insta::assert_snapshot!(render_to_string(&mut head, Size::new(50, 10)));
    }
}
