//! Session updates: create, pick, list notes, jump to one, copy the prompt.

use std::fs;
use std::path::Path;

use two_face::re_exports::syntect::highlighting::Theme;

use tuirealm::ratatui::layout::Rect;

use super::{Id, Model, ToastKind, layout};
use crate::anchor::relocate;
use crate::compile::compile;
use crate::components::code_view::CodeProps;
use crate::components::session_view;
use crate::components::session_view::{NoteRow, SNIPPET_LINES};
use crate::highlight::Highlighted;
use crate::model::{Note, Session, View, note_count};

impl Model {
    pub(super) fn new_session(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return self.toast(ToastKind::Warning, "A session needs a name");
        }
        let session = Session::new(name);
        let result = self.store.save_session(&session);
        self.saved(result);
        self.state.active_session_id = Some(session.id.clone());
        self.sessions.push(session);
        self.save_state();
        self.popup = None;
        self.toast(ToastKind::Info, format!("Created session \"{name}\""));
        self.notes_changed();
        if std::mem::take(&mut self.note_after_new_session) {
            self.add_note();
        }
    }

    pub(super) fn pick_session(&mut self, index: usize) {
        let Some(session) = self.sessions.get(index) else {
            return;
        };
        self.state.active_session_id = Some(session.id.clone());
        self.save_state();
        self.popup = None;
        self.note_cursor = 0;
        self.refresh_marks();
        self.show_view(View::Session);
    }

    /// Re-read the active session's files and relocate its notes.
    pub(super) fn resolve_notes(&mut self) {
        let notes = match self.active_session() {
            Some(s) => s
                .notes
                .iter()
                .map(|n| resolve(&self.root, n, &self.theme))
                .collect(),
            None => Vec::new(),
        };
        self.notes = notes;
        self.note_cursor = self.note_cursor.min(self.notes.len().saturating_sub(1));
    }

    /// What the Session view shows.
    pub fn session_props(&self) -> CodeProps {
        let focused = self.active_id() == Id::SessionView;
        let Some(session) = self.active_session() else {
            return CodeProps {
                focused,
                ..CodeProps::message(
                    "Session",
                    "No active session. Ctrl+N creates one, Ctrl+O picks one.",
                )
            };
        };
        let height = layout(Rect::new(0, 0, self.size.0, self.size.1))
            .main
            .height;
        let title = format!("Session: {}", session.name);
        session_view::build(
            &title,
            &self.notes,
            self.note_cursor,
            height.saturating_sub(2) as usize,
            focused,
        )
    }

    pub(super) fn jump_to_note(&mut self) {
        let Some(row) = self.notes.get(self.note_cursor) else {
            return;
        };
        let Some((file, span)) = row.jump.clone() else {
            return self.toast(ToastKind::Warning, "This note is stale: its code is gone");
        };
        let line = span.start_line as usize - 1;
        let col = span.start_col.map_or(0, |c| c as usize - 1);
        self.goto(file, line, col);
    }

    pub(super) fn copy_prompt(&mut self) {
        let Some(session) = self.active_session() else {
            return self.toast(
                ToastKind::Warning,
                "No active session: Ctrl+N creates one, Ctrl+O picks one",
            );
        };
        let (prompt, warnings) = compile(session, &self.root);
        let warning = warnings.first().map(|w| match warnings.len() {
            1 => w.to_string(),
            n => format!("{w} (+{} more)", n - 1),
        });
        if prompt.is_empty() {
            return self.toast(ToastKind::Warning, warning.unwrap_or_default());
        }
        let copied = session.notes.len() - warnings.len();
        self.clipboard = Some(prompt);
        match warning {
            Some(w) => self.toast(
                ToastKind::Warning,
                format!("Copied {} · {w}", note_count(copied)),
            ),
            None => self.toast(
                ToastKind::Info,
                format!("Copied prompt ({})", note_count(copied)),
            ),
        }
    }
}

fn resolve(root: &Path, note: &Note, theme: &Theme) -> NoteRow {
    let text = fs::read_to_string(root.join(&note.file)).ok();
    let span = text
        .as_deref()
        .and_then(|t| relocate(note.span, &note.anchor, t).span(note.span));
    let lines: Vec<String> = match (span, &text) {
        (Some(span), Some(text)) => text
            .lines()
            .skip(span.start_line as usize - 1)
            .take((span.end_line - span.start_line) as usize + 1)
            .map(str::to_string)
            .collect(),
        // Stale: show what was annotated.
        _ => note.anchor.selected.lines().map(str::to_string).collect(),
    };
    let first = lines.first().map_or("", String::as_str);
    let mut hl = Highlighted::new(&note.file, first, theme);
    hl.ensure(&lines, SNIPPET_LINES, theme);
    NoteRow {
        location: format!(
            "{}:{}",
            note.file.display(),
            span.unwrap_or(note.span).ui_label()
        ),
        snippet: hl.lines,
        more: lines.len().saturating_sub(SNIPPET_LINES),
        text: note.text.clone(),
        stale: span.is_none(),
        jump: span.map(|s| (note.file.clone(), s)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::capture;
    use crate::app::{Msg, Popup};
    use crate::highlight::load_theme;
    use crate::model::Span;
    use crate::store::MemStore;
    use crate::testutil::fixture_repo;

    fn model(root: &Path) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(root.to_path_buf(), theme, Box::new(MemStore::default())).unwrap();
        m.update(Msg::Resize(80, 20));
        m
    }

    fn note(root: &Path, file: &str, span: Span, text: &str) -> Note {
        let src = fs::read_to_string(root.join(file)).unwrap();
        Note::new(file.into(), span, capture(span, &src).unwrap(), text)
    }

    #[test]
    fn create_switch_and_copy() {
        let repo = fixture_repo();
        let mut m = model(repo.path());

        m.update(Msg::CopyPrompt);
        assert!(
            m.toast
                .as_ref()
                .unwrap()
                .text
                .starts_with("No active session")
        );

        m.update(Msg::OpenPopup(Popup::NewSession));
        m.update(Msg::NewSession("   ".into()));
        assert_eq!(
            m.popup,
            Some(Popup::NewSession),
            "empty name keeps the popup open"
        );
        m.update(Msg::NewSession("Perf".into()));
        m.update(Msg::NewSession("Docs".into()));
        assert_eq!(m.popup, None);
        assert_eq!(m.active_session().unwrap().name, "Docs");
        // Autosaved.
        let saved = m.store.load_sessions().unwrap();
        assert_eq!(
            saved.iter().map(|s| &s.name).collect::<Vec<_>>(),
            ["Perf", "Docs"]
        );
        assert_eq!(
            m.store.load_state().unwrap().active_session_id,
            Some(saved[1].id.clone())
        );

        // Empty session: nothing copied, a warning instead.
        m.update(Msg::CopyPrompt);
        assert_eq!(m.clipboard, None);
        assert_eq!(m.toast.as_ref().unwrap().kind, ToastKind::Warning);

        // Give "Perf" a note, switch to it with the picker, copy.
        let n = note(
            repo.path(),
            "src/matcher.rs",
            Span::lines(3, 7),
            "use an iterator",
        );
        m.sessions[0].notes.push(n);
        m.update(Msg::OpenPopup(Popup::SessionPicker));
        m.update(Msg::PickSession(0));
        assert_eq!((m.popup, m.view), (None, View::Session));
        assert_eq!(m.active_session().unwrap().name, "Perf");
        assert_eq!(m.notes.len(), 1);
        assert_eq!(m.notes[0].location, "src/matcher.rs:L3-7");

        m.update(Msg::CopyPrompt);
        let (expected, _) = compile(&m.sessions[0], repo.path());
        assert_eq!(m.clipboard.as_deref(), Some(expected.as_str()));
        assert_eq!(m.toast.as_ref().unwrap().text, "Copied prompt (1 note)");
    }

    #[test]
    fn session_view_lists_and_jumps() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("Perf".into()));
        let notes = vec![
            note(repo.path(), "src/matcher.rs", Span::lines(3, 7), "iterator"),
            note(
                repo.path(),
                "scripts/score.py",
                Span::chars(2, 13, 2, 38),
                "fsum",
            ),
            note(repo.path(), "web/app.ts", Span::lines(1, 1), "gone soon"),
        ];
        m.sessions[0].notes = notes;
        fs::write(repo.path().join("web/app.ts"), "// rewritten\n").unwrap();
        m.update(Msg::ShowView(View::Session));
        assert_eq!(
            m.notes
                .iter()
                .map(|n| (n.location.as_str(), n.stale))
                .collect::<Vec<_>>(),
            [
                ("src/matcher.rs:L3-7", false),
                ("scripts/score.py:L2:13-L2:38", false),
                ("web/app.ts:L1", true),
            ]
        );

        press(&mut m, &[Msg::NoteDown, Msg::NoteDown, Msg::NoteDown]);
        assert_eq!(m.note_cursor, 2);
        m.update(Msg::JumpToNote);
        assert_eq!(m.view, View::Session, "stale notes can't be jumped to");

        press(&mut m, &[Msg::NoteUp, Msg::JumpToNote]);
        assert_eq!(m.view, View::Files);
        assert_eq!(m.location().unwrap(), "scripts/score.py 2:13");
    }

    #[test]
    fn jump_centers_the_target_line() {
        let repo = fixture_repo();
        let body: String = (1..=200).map(|i| format!("line {i}\n")).collect();
        fs::write(repo.path().join("long.txt"), body).unwrap();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        m.sessions[0]
            .notes
            .push(note(repo.path(), "long.txt", Span::lines(100, 101), "x"));
        press(&mut m, &[Msg::ShowView(View::Session), Msg::JumpToNote]);
        // 20 rows: 1 status bar, 2 borders → 17 visible lines.
        assert_eq!((m.cursor.line, m.top), (99, 99 - 8));
    }

    #[test]
    fn state_survives_restart() {
        let repo = fixture_repo();
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let store = crate::store::FsStore::new(repo.path());
        let mut m = Model::new(repo.path().into(), theme.clone(), Box::new(store)).unwrap();
        m.update(Msg::NewSession("Perf".into()));
        m.update(Msg::OpenFile("web/app.ts".into()));
        drop(m);
        let store = crate::store::FsStore::new(repo.path());
        let m = Model::new(repo.path().into(), theme, Box::new(store)).unwrap();
        assert_eq!(m.active_session().unwrap().name, "Perf");
        assert_eq!(m.location().unwrap(), "web/app.ts 1:1");
    }

    fn press(m: &mut Model, msgs: &[Msg]) {
        msgs.iter().for_each(|msg| m.update(msg.clone()));
    }
}
