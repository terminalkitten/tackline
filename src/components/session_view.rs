//! Session view: the active session's notes, in prompt order.

use std::path::PathBuf;

use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};

use super::code_view::{CodeProps, Rows};
use super::{global_key, key};
use crate::app::Msg;
use crate::model::Span as NoteSpan;

/// Snippet lines shown per note; the rest collapse into "… N more lines".
pub const SNIPPET_LINES: usize = 6;

/// One note as shown in the list, resolved against the file on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteRow {
    /// `src/match.rs:L88-95`
    pub location: String,
    /// Highlighted, at most `SNIPPET_LINES`.
    pub snippet: Vec<Line<'static>>,
    /// Snippet lines not shown.
    pub more: usize,
    pub text: String,
    pub stale: bool,
    /// Where to jump; `None` when stale.
    pub jump: Option<(PathBuf, NoteSpan)>,
}

pub fn build(
    title: &str,
    notes: &[NoteRow],
    selected: usize,
    height: usize,
    focused: bool,
) -> CodeProps {
    if notes.is_empty() {
        return CodeProps {
            focused,
            ..CodeProps::message(
                title,
                "No notes yet. Select code in the Files view and press c.",
            )
        };
    }
    let dim = Style::new().fg(Color::DarkGray);
    let mut rows = Vec::new();
    let mut selected_rows = 0..0;
    for (i, note) in notes.iter().enumerate() {
        let start = rows.len();
        let bar = if i == selected {
            Span::styled("▌ ", Style::new().fg(Color::Cyan))
        } else {
            Span::raw("  ")
        };
        let mut head = vec![bar.clone(), Span::raw(format!("{}. ", i + 1))];
        if note.stale {
            head.push(Span::styled("⚠ stale ", Style::new().fg(Color::Yellow)));
        }
        let location = Style::new().add_modifier(Modifier::BOLD);
        head.push(Span::styled(
            note.location.clone(),
            if note.stale { dim } else { location },
        ));
        rows.push(Line::from(head));
        for line in &note.snippet {
            let mut spans = vec![bar.clone(), Span::styled("   │ ", dim)];
            spans.extend(line.spans.iter().cloned());
            rows.push(Line::from(spans));
        }
        if note.more > 0 {
            let more = format!("   │ … {} more lines", note.more);
            rows.push(Line::from(vec![bar.clone(), Span::styled(more, dim)]));
        }
        for line in note.text.lines() {
            rows.push(Line::from(vec![
                bar.clone(),
                Span::raw(format!("   {line}")),
            ]));
        }
        if i == selected {
            selected_rows = start..rows.len();
        }
        rows.push(Line::default());
    }
    // Scroll just enough to show the whole selected note (or its top).
    let scroll = selected_rows
        .end
        .saturating_sub(height)
        .min(selected_rows.start);
    CodeProps {
        title: title.to_string(),
        rows,
        focused,
        scroll: scroll as u16,
    }
}

#[derive(Component, Default)]
pub struct SessionView {
    component: Rows,
}

impl AppComponent<Msg, NoUserEvent> for SessionView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        match key(ev)? {
            (Key::Down | Key::Char('j'), false) => Some(Msg::NoteDown),
            (Key::Up | Key::Char('k'), false) => Some(Msg::NoteUp),
            (Key::Enter, false) => Some(Msg::JumpToNote),
            (Key::Char('y'), false) => Some(Msg::CopyPrompt),
            (Key::Char('e'), false) => Some(Msg::EditNote),
            (Key::Char('d'), false) => Some(Msg::AskDeleteNote),
            (Key::Char('J'), false) => Some(Msg::MoveNote(1)),
            (Key::Char('K'), false) => Some(Msg::MoveNote(-1)),
            _ => global_key(ev),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::props::{AttrValue, Attribute, PropBound, PropPayload};
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    fn row(location: &str, snippet: &[&str], more: usize, text: &str, stale: bool) -> NoteRow {
        NoteRow {
            location: location.into(),
            snippet: snippet.iter().map(|l| Line::raw(l.to_string())).collect(),
            more,
            text: text.into(),
            stale,
            jump: None,
        }
    }

    fn render(props: CodeProps, size: Size) -> String {
        let mut view = SessionView::default();
        view.attr(
            Attribute::Value,
            AttrValue::Payload(PropPayload::Any(props.to_any_prop())),
        );
        render_to_string(&mut view, size)
    }

    #[test]
    fn renders_notes() {
        let notes = [
            row(
                "src/match.rs:L88-95",
                &["fn a() {", "    b();"],
                6,
                "make this async,\nkeep the signature",
                false,
            ),
            row(
                "src/score.rs:L12:4-L12:30",
                &["let s = x;"],
                0,
                "use the helper",
                false,
            ),
            row("web/app.ts:L1", &["export {}"], 0, "gone", true),
        ];
        let props = build("Session: Perf", &notes, 1, 20, true);
        insta::assert_snapshot!(render(props, Size::new(50, 18)));
    }

    #[test]
    fn scrolls_to_selected_note() {
        let notes: Vec<_> = (1..=10)
            .map(|i| row(&format!("f{i}.rs:L1"), &["x"], 0, "do", false))
            .collect();
        // Each note is 4 rows; with 10 visible rows, note 6 (rows 20..23) needs scroll 13.
        assert_eq!(build("S", &notes, 5, 10, true).scroll, 13);
        assert_eq!(build("S", &notes, 0, 10, true).scroll, 0);
    }

    #[test]
    fn empty_session() {
        let props = build("Session: Perf", &[], 0, 10, true);
        insta::assert_snapshot!(render(props, Size::new(70, 3)));
    }
}
