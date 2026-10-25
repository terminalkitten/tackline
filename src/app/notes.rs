//! Notes: selecting code, the note draft, editing the session's notes, and
//! the gutter marks for the open file.

use std::path::PathBuf;

use super::{Cursor, Model, Motion, Popup, ToastKind, display_col};
use crate::anchor::{Relocation, capture, relocate};
use crate::components::code_view::{CodeProps, Mark, Selection as SelectionView};
use crate::components::note_popup;
use crate::git::Row;
use crate::model::{Anchor, Note, Span, View};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Char,
    Line,
}

/// From `anchor` (where it started) to the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub kind: SelectionKind,
    pub anchor: Cursor,
}

/// A note being written or edited in the note popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub file: PathBuf,
    pub span: Span,
    pub anchor: Anchor,
    /// Starting text (empty for a new note).
    pub text: String,
    /// Index into `sessions`.
    pub target: usize,
    /// Id of the note being edited.
    pub editing: Option<String>,
}

impl Model {
    /// `v` / `V`: start a selection, switch its kind, or end it.
    pub(super) fn toggle_selection(&mut self, kind: SelectionKind) {
        self.selection = match self.selection {
            Some(s) if s.kind == kind => None,
            Some(s) => Some(Selection { kind, ..s }),
            None => Some(Selection {
                kind,
                anchor: self.cursor,
            }),
        };
    }

    /// Shift+arrow: start a selection if there isn't one, then move.
    pub(super) fn extend_selection(&mut self, kind: SelectionKind, motion: Motion) {
        if self.selection.is_none() {
            self.selection = Some(Selection {
                kind,
                anchor: self.cursor,
            });
        }
        self.move_cursor(motion);
    }

    /// The selection as a note span, or the cursor line without one.
    fn selected_span(&self) -> Span {
        let Some(sel) = self.selection else {
            let line = self.cursor.line as u32 + 1;
            return Span::lines(line, line);
        };
        let (a, b) = (sel.anchor, self.cursor);
        let (start, end) = if (a.line, a.col) <= (b.line, b.col) {
            (a, b)
        } else {
            (b, a)
        };
        match sel.kind {
            SelectionKind::Line => Span::lines(start.line as u32 + 1, end.line as u32 + 1),
            SelectionKind::Char => Span::chars(
                start.line as u32 + 1,
                start.col as u32 + 1,
                end.line as u32 + 1,
                end.col as u32 + 1,
            ),
        }
    }

    /// The selection for drawing, in display columns.
    pub(super) fn selection_view(&self) -> Option<SelectionView> {
        self.selection?;
        let lines = self.lines()?;
        let span = self.selected_span();
        let at = |line: u32, col: Option<u32>| {
            let line = line as usize - 1;
            (
                line,
                display_col(&lines[line], col.map_or(0, |c| c as usize - 1)),
            )
        };
        Some(SelectionView {
            start: at(span.start_line, span.start_col),
            end: at(span.end_line, span.end_col),
            linewise: span.cols().is_none(),
        })
    }

    /// `c`: open the note popup for the selection (or the cursor line).
    pub(super) fn add_note(&mut self) {
        let (Some(file), Some(lines)) = (&self.file, self.lines()) else {
            return self.toast(ToastKind::Warning, "Open a file to add a note");
        };
        if self.sessions.is_empty() {
            // Name a session first; `new_session` comes back here.
            self.note_after_new_session = true;
            self.popup = Some(Popup::NewSession);
            return;
        }
        let span = self.selected_span();
        // Code that already has a note: open that note instead.
        let lines_of = |m: &Mark| (m.start as u32 + 1, m.end as u32 + 1);
        let existing = self
            .marks
            .iter()
            .filter(|m| !m.stale)
            .find(|m| match self.selection {
                None => (m.start..=m.end).contains(&self.cursor.line),
                Some(_) => lines_of(m) == (span.start_line, span.end_line),
            });
        if let Some(mark) = existing {
            let index = mark.note;
            self.selection = None;
            self.resolve_notes();
            return self.edit_note_at(index);
        }
        let path = file.path.clone();
        // In a diff, rows map to the working tree's (new-side) lines.
        let (span, text) = match &file.diff {
            None => (span, lines.join("\n")),
            Some(rows) => {
                let picked = &rows[span.start_line as usize - 1..span.end_line as usize];
                let Some(new) = picked.iter().map(|r| r.new).collect::<Option<Vec<u32>>>() else {
                    return self.toast(
                        ToastKind::Warning,
                        "Removed lines and hunk headers can't have notes: select new-side lines",
                    );
                };
                let Ok(text) = std::fs::read_to_string(self.root.join(&path)) else {
                    return self.toast(ToastKind::Error, "Can't read the file for this diff");
                };
                let span = Span {
                    start_line: new[0],
                    end_line: new[new.len() - 1],
                    ..span
                };
                (span, text)
            }
        };
        let Some(anchor) = capture(span, &text) else {
            return;
        };
        self.draft = Some(Draft {
            file: path,
            span,
            anchor,
            text: String::new(),
            target: self.active_index().unwrap_or(0),
            editing: None,
        });
        self.popup = Some(Popup::Note);
    }

    pub(super) fn active_index(&self) -> Option<usize> {
        let id = self.state.active_session_id.as_ref()?;
        self.sessions.iter().position(|s| &s.id == id)
    }

    /// Tab / Shift+Tab in the note popup: change the target session. For a
    /// note being edited, saving then moves it there.
    pub(super) fn cycle_session(&mut self, step: isize) {
        let len = self.sessions.len() as isize;
        if let Some(draft) = self.draft.as_mut() {
            draft.target = (draft.target as isize + step).rem_euclid(len) as usize;
        }
    }

    pub(super) fn save_note(&mut self, text: &str) {
        let Some(draft) = self.draft.take() else {
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            self.draft = Some(draft);
            return self.toast(ToastKind::Warning, "The note needs some text");
        }
        let mut verb = "Added note to";
        match &draft.editing {
            Some(id) => {
                let Some((from, i)) = self.find_note(id) else {
                    return;
                };
                if from == draft.target {
                    self.sessions[from].notes[i].text = text.to_string();
                    verb = "Updated note in";
                } else {
                    // Moved: out of its session, to the end of the target.
                    let mut note = self.sessions[from].notes.remove(i);
                    note.text = text.to_string();
                    self.sessions[draft.target].notes.push(note);
                    let result = self.store.save_session(&self.sessions[from]);
                    self.saved(result);
                    verb = "Moved note to";
                }
            }
            None => self.sessions[draft.target].notes.push(Note::new(
                draft.file,
                draft.span,
                draft.anchor,
                text,
            )),
        }
        let name = self.sessions[draft.target].name.clone();
        let result = self.store.save_session(&self.sessions[draft.target]);
        self.saved(result);
        self.popup = None;
        self.selection = None;
        self.notes_changed();
        self.toast(ToastKind::Info, format!("{verb} \"{name}\""));
    }

    /// (session index, note index) of the note with this id.
    fn find_note(&self, id: &str) -> Option<(usize, usize)> {
        self.sessions.iter().enumerate().find_map(|(s, session)| {
            let i = session.notes.iter().position(|n| n.id == id)?;
            Some((s, i))
        })
    }

    /// The text of the note under the cursor, on one line and cut to
    /// `max` chars with `…`.
    pub(super) fn note_excerpt(&self, max: usize) -> Option<String> {
        let line = self.cursor.line;
        let mark = self
            .marks
            .iter()
            .find(|m| (m.start..=m.end).contains(&line))?;
        let text = &self.active_session()?.notes.get(mark.note)?.text;
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.chars().count() <= max {
            return Some(flat);
        }
        let cut: String = flat.chars().take(max.saturating_sub(1)).collect();
        Some(format!("{}…", cut.trim_end()))
    }

    /// Enter: show the note under the cursor in full.
    pub(super) fn view_note(&mut self) {
        let line = self.cursor.line;
        let Some(mark) = self
            .marks
            .iter()
            .find(|m| (m.start..=m.end).contains(&line))
        else {
            return self.toast(ToastKind::Warning, "No note on this line (c adds one)");
        };
        self.note_cursor = mark.note;
        self.resolve_notes();
        self.popup = Some(Popup::ViewNote);
    }

    /// The note viewer: location, session and the full text, wrapped.
    pub fn note_view_props(&self) -> Option<CodeProps> {
        let session = self.active_session()?;
        let note = session.notes.get(self.note_cursor)?;
        let row = self.notes.get(self.note_cursor)?;
        let screen = tuirealm::ratatui::layout::Rect::new(0, 0, self.size.0, self.size.1);
        let width = note_popup::viewer_width(screen).saturating_sub(4) as usize;
        Some(note_popup::viewer(
            &row.location,
            &session.name,
            &note.text,
            row.stale,
            width,
        ))
    }

    /// The note popup's header.
    pub fn note_header(&self) -> Option<CodeProps> {
        let draft = self.draft.as_ref()?;
        let title = if draft.editing.is_some() {
            "Edit note"
        } else {
            "Add note"
        };
        let location = format!("{}:{}", draft.file.display(), draft.span.ui_label());
        let can_cycle = self.sessions.len() > 1;
        let session = &self.sessions.get(draft.target)?.name;
        Some(note_popup::header(
            title,
            &location,
            &draft.anchor.selected,
            session,
            can_cycle,
        ))
    }

    /// `e` in the Session view.
    pub(super) fn edit_note(&mut self) {
        self.edit_note_at(self.note_cursor);
    }

    /// Open the active session's note `index` in the note popup.
    fn edit_note_at(&mut self, index: usize) {
        let Some(target) = self.active_index() else {
            return;
        };
        let Some(note) = self.sessions[target].notes.get(index) else {
            return;
        };
        self.draft = Some(Draft {
            file: note.file.clone(),
            span: self
                .notes
                .get(index)
                .and_then(|row| row.jump.as_ref())
                .map_or(note.span, |(_, s)| *s),
            anchor: note.anchor.clone(),
            text: note.text.clone(),
            target,
            editing: Some(note.id.clone()),
        });
        self.popup = Some(Popup::Note);
    }

    /// `d` in the Session view (asks first).
    /// `d`: in the Session view the selected note, in the code view the
    /// note under the cursor (asks first).
    pub(super) fn ask_delete_note(&mut self) {
        if matches!(self.view, View::Files | View::Diff) {
            let line = self.cursor.line;
            let Some(mark) = self
                .marks
                .iter()
                .find(|m| (m.start..=m.end).contains(&line))
            else {
                return self.toast(ToastKind::Warning, "No note on this line");
            };
            self.note_cursor = mark.note;
            self.resolve_notes();
        }
        if !self.notes.is_empty() {
            self.popup = Some(Popup::ConfirmDelete);
        }
    }

    pub(super) fn delete_note(&mut self) {
        self.popup = None;
        let Some(target) = self.active_index() else {
            return;
        };
        if self.note_cursor >= self.sessions[target].notes.len() {
            return;
        }
        self.sessions[target].notes.remove(self.note_cursor);
        let result = self.store.save_session(&self.sessions[target]);
        self.saved(result);
        self.notes_changed();
        self.toast(ToastKind::Info, "Deleted note");
    }

    /// `J` / `K` in the Session view: move the selected note down / up.
    pub(super) fn move_note(&mut self, step: isize) {
        let Some(target) = self.active_index() else {
            return;
        };
        let notes = &mut self.sessions[target].notes;
        let to = self.note_cursor as isize + step;
        if to < 0 || to as usize >= notes.len() {
            return;
        }
        notes.swap(self.note_cursor, to as usize);
        self.note_cursor = to as usize;
        let result = self.store.save_session(&self.sessions[target]);
        self.saved(result);
        self.notes_changed();
    }

    /// After notes change: refresh the Session view list and the gutter.
    pub(super) fn notes_changed(&mut self) {
        self.resolve_notes();
        self.refresh_marks();
    }

    /// Relocate the active session's notes on the open file for the gutter.
    pub(super) fn refresh_marks(&mut self) {
        let marks = match (&self.file, self.lines(), self.active_session()) {
            (Some(file), Some(lines), Some(session)) => {
                // A diff relocates against the file on disk, then maps the
                // new-side lines to diff rows.
                let text = match &file.diff {
                    None => lines.join("\n"),
                    Some(_) => {
                        std::fs::read_to_string(self.root.join(&file.path)).unwrap_or_default()
                    }
                };
                session
                    .notes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.file == file.path)
                    .filter_map(|(i, n)| {
                        let (span, stale) = match relocate(n.span, &n.anchor, &text) {
                            Relocation::Kept => (n.span, false),
                            Relocation::Moved(span) => (span, false),
                            Relocation::Stale => (n.span, true),
                        };
                        let (start, end) = match &file.diff {
                            None => (span.start_line as usize - 1, span.end_line as usize - 1),
                            Some(rows) => {
                                let shown = |r: &&Row| {
                                    r.new.is_some_and(|n| {
                                        (span.start_line..=span.end_line).contains(&n)
                                    })
                                };
                                let first = rows.iter().position(|r| shown(&r))?;
                                let last = rows.iter().rposition(|r| shown(&r))?;
                                (first, last)
                            }
                        };
                        Some(Mark {
                            start,
                            end,
                            stale,
                            note: i,
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        self.marks = marks;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Msg, ToastKind};
    use crate::compile::compile;
    use crate::highlight::load_theme;
    use crate::model::View;
    use crate::store::MemStore;
    use crate::testutil::fixture_repo;
    use std::fs;
    use std::path::Path;

    fn model(root: &Path) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(root.to_path_buf(), theme, Box::new(MemStore::default())).unwrap();
        m.update(Msg::Resize(80, 20));
        m
    }

    fn press(m: &mut Model, msgs: &[Msg]) {
        msgs.iter().for_each(|msg| m.update(msg.clone()));
    }

    /// The milestone's "done when": select → note → see in session → copy.
    #[test]
    fn select_note_see_copy() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        // No session yet: `c` asks for a name first, then opens the note.
        press(
            &mut m,
            &[
                Msg::OpenFile("src/matcher.rs".into()),
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        assert_eq!(m.popup, Some(Popup::NewSession));
        m.update(Msg::NewSession("Perf".into()));
        assert_eq!(m.popup, Some(Popup::Note));
        let draft = m.draft.clone().unwrap();
        assert_eq!(draft.span, Span::lines(1, 3));

        m.update(Msg::SaveNote("  use an iterator chain \n".into()));
        assert_eq!((m.popup, m.selection), (None, None));
        let session = m.active_session().unwrap();
        assert_eq!(session.notes.len(), 1);
        assert_eq!(session.notes[0].text, "use an iterator chain");
        assert_eq!(session.notes[0].span, Span::lines(1, 3));
        assert_eq!(
            m.marks,
            [Mark {
                start: 0,
                end: 2,
                stale: false,
                note: 0,
            }]
        );

        m.update(Msg::ShowView(View::Session));
        assert_eq!(m.notes[0].location, "src/matcher.rs:L1-3");

        m.update(Msg::CopyPrompt);
        let (expected, _) = compile(m.active_session().unwrap(), repo.path());
        assert!(expected.contains("use an iterator chain"));
        assert_eq!(m.clipboard.as_deref(), Some(expected.as_str()));
        // Autosaved.
        assert_eq!(m.store.load_sessions().unwrap()[0].notes.len(), 1);
    }

    #[test]
    fn charwise_and_shift_selection() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        // `out.push(item.clone());` is line 5, starting at col 13.
        press(
            &mut m,
            &[
                Msg::OpenFile("src/matcher.rs".into()),
                Msg::Move(Motion::Down),
            ],
        );
        for _ in 0..3 {
            m.update(Msg::Move(Motion::Down));
        }
        for _ in 0..12 {
            m.update(Msg::Move(Motion::Right));
        }
        m.update(Msg::ToggleSelection(SelectionKind::Char));
        for _ in 0..7 {
            m.update(Msg::Move(Motion::Right));
        }
        m.update(Msg::AddNote);
        let draft = m.draft.clone().unwrap();
        assert_eq!(draft.span, Span::chars(5, 13, 5, 20));
        assert_eq!(draft.anchor.selected, "out.push");
        m.update(Msg::ClosePopup);
        assert_eq!(m.draft, None);

        // Shift+Down from scratch makes a linewise selection.
        press(&mut m, &[Msg::ClearSelection, Msg::Move(Motion::Top)]);
        press(
            &mut m,
            &[
                Msg::ExtendSelection(SelectionKind::Line, Motion::Down),
                Msg::ExtendSelection(SelectionKind::Line, Motion::Down),
                Msg::AddNote,
            ],
        );
        assert_eq!(m.draft.as_ref().unwrap().span, Span::lines(1, 3));
        // Upwards works too: the span is ordered.
        press(
            &mut m,
            &[
                Msg::ClosePopup,
                Msg::ClearSelection,
                Msg::Move(Motion::Bottom),
                Msg::ExtendSelection(SelectionKind::Line, Motion::Up),
                Msg::AddNote,
            ],
        );
        assert_eq!(m.draft.as_ref().unwrap().span, Span::lines(8, 9));
    }

    #[test]
    fn c_on_noted_code_edits_that_note() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        press(&mut m, &[Msg::Move(Motion::Down), Msg::Move(Motion::Down)]);
        press(
            &mut m,
            &[
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        m.update(Msg::SaveNote("first".into()));

        // Cursor inside the note, no selection: edit it.
        m.update(Msg::AddNote);
        let draft = m.draft.clone().unwrap();
        assert_eq!(
            (draft.text.as_str(), draft.span),
            ("first", Span::lines(3, 4))
        );
        assert!(draft.editing.is_some());
        m.update(Msg::SaveNote("changed".into()));
        assert_eq!(m.active_session().unwrap().notes.len(), 1);
        assert_eq!(m.active_session().unwrap().notes[0].text, "changed");

        // The same lines selected again: edit too.
        press(
            &mut m,
            &[
                Msg::Move(Motion::Up),
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        assert_eq!(m.draft.as_ref().unwrap().text, "changed");
        m.update(Msg::ClosePopup);

        // A different selection overlapping it: a new note.
        press(
            &mut m,
            &[
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        assert!(m.draft.as_ref().unwrap().editing.is_none());
        assert_eq!(m.draft.as_ref().unwrap().span, Span::lines(4, 5));
    }

    #[test]
    fn d_in_code_view_deletes_the_note_under_the_cursor() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        press(&mut m, &[Msg::AddNote, Msg::SaveNote("first line".into())]);
        press(
            &mut m,
            &[
                Msg::Move(Motion::Bottom),
                Msg::AddNote,
                Msg::SaveNote("last line".into()),
            ],
        );
        m.update(Msg::Move(Motion::Up));
        m.update(Msg::AskDeleteNote);
        assert_eq!(m.popup, None, "no note on line 8");
        assert_eq!(m.toast.as_ref().unwrap().text, "No note on this line");
        press(&mut m, &[Msg::Move(Motion::Bottom), Msg::AskDeleteNote]);
        assert_eq!(m.popup, Some(Popup::ConfirmDelete));
        m.update(Msg::DeleteNote);
        let texts: Vec<_> = m
            .active_session()
            .unwrap()
            .notes
            .iter()
            .map(|n| n.text.clone())
            .collect();
        assert_eq!(texts, ["first line"]);
        assert_eq!(m.marks.len(), 1);
    }

    #[test]
    fn excerpt_of_the_note_under_the_cursor() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        press(
            &mut m,
            &[
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::AddNote,
                Msg::SaveNote("replace the loop\nwith an   iterator chain".into()),
            ],
        );
        assert_eq!(
            m.note_excerpt(80).as_deref(),
            Some("replace the loop with an iterator chain")
        );
        assert_eq!(m.note_excerpt(12).as_deref(), Some("replace the…"));
        m.update(Msg::Move(Motion::Bottom));
        assert_eq!(m.note_excerpt(80), None, "no note on this line");
        // The pane shows it in its bottom border.
        m.update(Msg::Move(Motion::Top));
        assert_eq!(
            m.code_props().footer.as_deref(),
            Some("replace the loop with an iterator chain")
        );
    }

    #[test]
    fn enter_views_the_whole_note() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::ViewNote);
        assert_eq!(m.popup, None);
        assert!(
            m.toast
                .as_ref()
                .unwrap()
                .text
                .starts_with("No note on this line")
        );

        let text = "replace the loop with an iterator chain and keep the order of items stable";
        press(
            &mut m,
            &[
                Msg::Move(Motion::Down),
                Msg::AddNote,
                Msg::SaveNote(text.into()),
            ],
        );
        m.update(Msg::ViewNote);
        assert_eq!(m.popup, Some(Popup::ViewNote));
        let view = m.note_view_props().unwrap();
        let rows: Vec<String> = view.rows.iter().map(|r| r.to_string()).collect();
        assert_eq!(rows[0], "src/matcher.rs:L2");
        assert_eq!(rows[1], "Session: S");
        // The full text, wrapped, nothing cut off.
        let body: String = rows[3..rows.len() - 2]
            .iter()
            .map(|r| r.trim())
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(body, text);

        // e edits the same note; d asks to delete it.
        m.update(Msg::EditNote);
        assert_eq!(m.popup, Some(Popup::Note));
        assert_eq!(m.draft.as_ref().unwrap().text, text);
        press(
            &mut m,
            &[Msg::ClosePopup, Msg::ViewNote, Msg::AskDeleteNote],
        );
        assert_eq!(m.popup, Some(Popup::ConfirmDelete));
    }

    #[test]
    fn note_on_cursor_line_without_selection() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        press(
            &mut m,
            &[
                Msg::OpenFile("web/app.ts".into()),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        assert_eq!(m.draft.as_ref().unwrap().span, Span::lines(2, 2));
    }

    #[test]
    fn target_session_cycles_and_active_stays() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        press(
            &mut m,
            &[Msg::NewSession("A".into()), Msg::NewSession("B".into())],
        );
        press(&mut m, &[Msg::OpenFile("web/app.ts".into()), Msg::AddNote]);
        assert_eq!(
            m.draft.as_ref().unwrap().target,
            1,
            "defaults to the active session"
        );
        press(&mut m, &[Msg::CycleSession(1)]);
        assert_eq!(m.draft.as_ref().unwrap().target, 0, "wraps");
        press(
            &mut m,
            &[
                Msg::CycleSession(-1),
                Msg::CycleSession(-1),
                Msg::SaveNote("x".into()),
            ],
        );
        assert_eq!(m.sessions[0].notes.len(), 1);
        assert_eq!(
            m.active_session().unwrap().name,
            "B",
            "active session unchanged"
        );
        // The gutter shows only the active session's notes.
        assert!(m.marks.is_empty());
    }

    #[test]
    fn shift_tab_while_editing_moves_the_note() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        press(
            &mut m,
            &[Msg::NewSession("A".into()), Msg::NewSession("B".into())],
        );
        m.update(Msg::PickSession(0));
        m.update(Msg::OpenFile("web/app.ts".into()));
        press(&mut m, &[Msg::AddNote, Msg::SaveNote("keep".into())]);
        press(
            &mut m,
            &[
                Msg::Move(Motion::Down),
                Msg::AddNote,
                Msg::SaveNote("move me".into()),
            ],
        );
        let id = m.sessions[0].notes[1].id.clone();

        // `c` on the noted line opens it; Shift+Tab picks B; save moves it.
        m.update(Msg::AddNote);
        assert!(m.draft.as_ref().unwrap().editing.is_some());
        m.update(Msg::CycleSession(-1));
        assert_eq!(m.draft.as_ref().unwrap().target, 1);
        m.update(Msg::SaveNote("moved".into()));

        let texts = |m: &Model, s: usize| {
            m.sessions[s]
                .notes
                .iter()
                .map(|n| n.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&m, 0), ["keep"]);
        assert_eq!(texts(&m, 1), ["moved"]);
        assert_eq!(m.sessions[1].notes[0].id, id, "same note, moved");
        assert_eq!(m.toast.as_ref().unwrap().text, "Moved note to \"B\"");
        assert_eq!(
            m.active_session().unwrap().name,
            "A",
            "active session unchanged"
        );
        assert_eq!(m.marks.len(), 1, "gutter shows only A's note now");
        // Both sessions were saved.
        let saved = m.store.load_sessions().unwrap();
        assert_eq!((saved[0].notes.len(), saved[1].notes.len()), (1, 1));
    }

    #[test]
    fn empty_text_keeps_popup() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        press(
            &mut m,
            &[
                Msg::NewSession("S".into()),
                Msg::OpenFile("web/app.ts".into()),
                Msg::AddNote,
            ],
        );
        m.update(Msg::SaveNote(" \n ".into()));
        assert_eq!(m.popup, Some(Popup::Note));
        assert_eq!(m.toast.as_ref().unwrap().kind, ToastKind::Warning);
    }

    #[test]
    fn edit_delete_reorder_in_session_view() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        for (file, text) in [
            ("web/app.ts", "one"),
            ("src/matcher.rs", "two"),
            ("scripts/score.py", "three"),
        ] {
            press(
                &mut m,
                &[
                    Msg::OpenFile(file.into()),
                    Msg::AddNote,
                    Msg::SaveNote(text.into()),
                ],
            );
        }
        m.update(Msg::ShowView(View::Session));
        let texts = |m: &Model| {
            m.active_session()
                .unwrap()
                .notes
                .iter()
                .map(|n| n.text.clone())
                .collect::<Vec<_>>()
        };

        press(&mut m, &[Msg::NoteDown, Msg::EditNote]);
        assert_eq!(m.draft.as_ref().unwrap().text, "two");
        m.update(Msg::CycleSession(1));
        assert_eq!(
            m.draft.as_ref().unwrap().target,
            0,
            "one session: nowhere to move"
        );
        m.update(Msg::SaveNote("TWO".into()));
        assert_eq!(texts(&m), ["one", "TWO", "three"]);

        press(&mut m, &[Msg::MoveNote(1)]);
        assert_eq!(
            (texts(&m), m.note_cursor),
            (vec!["one".into(), "three".into(), "TWO".into()], 2)
        );
        press(&mut m, &[Msg::MoveNote(1)]);
        assert_eq!(m.note_cursor, 2, "can't move past the end");
        press(&mut m, &[Msg::NoteUp, Msg::NoteUp, Msg::MoveNote(-1)]);
        assert_eq!(m.note_cursor, 0);

        m.update(Msg::AskDeleteNote);
        assert_eq!(m.popup, Some(Popup::ConfirmDelete));
        m.update(Msg::ClosePopup);
        assert_eq!(texts(&m).len(), 3, "kept");
        press(&mut m, &[Msg::AskDeleteNote, Msg::DeleteNote]);
        assert_eq!(texts(&m), ["three", "TWO"]);
        assert_eq!(m.notes.len(), 2);
        assert_eq!(m.store.load_sessions().unwrap()[0].notes.len(), 2);
    }

    #[test]
    fn stale_note_marks_gutter() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("S".into()));
        press(
            &mut m,
            &[
                Msg::OpenFile("web/app.ts".into()),
                Msg::Move(Motion::Down),
                Msg::AddNote,
                Msg::SaveNote("x".into()),
            ],
        );
        fs::write(
            repo.path().join("web/app.ts"),
            "export const a = 1;\nexport const b = 2;\n",
        )
        .unwrap();
        m.update(Msg::OpenFile("web/app.ts".into()));
        assert_eq!(
            m.marks,
            [Mark {
                start: 1,
                end: 1,
                stale: true,
                note: 0,
            }]
        );
    }
}
