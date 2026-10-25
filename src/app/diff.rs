//! Diff view: the changed files and the diff of the selected one.

use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};

use super::{Content, Cursor, Id, Model, OpenFile, ToastKind};
use crate::components::code_view::CodeProps;
use crate::git;
use crate::model::View;

impl Model {
    /// Turn on the Diff view (inside a git repo). Starts in it when there
    /// are changes.
    pub fn with_git(mut self, base: Option<String>) -> Self {
        self.git = true;
        self.base = base;
        self.refresh_diff();
        if !self.diff.is_empty() {
            self.show_view(View::Diff);
        }
        self
    }

    /// `r`: reload the diff, staying on the same file and line if they're
    /// still there.
    pub(super) fn refresh_diff(&mut self) {
        if !self.git {
            return;
        }
        let selected = self.diff.get(self.diff_selected).map(|f| f.path.clone());
        match git::diff(&self.root, self.base.as_deref()) {
            Ok(files) => self.diff = files,
            Err(e) => {
                self.diff.clear();
                self.toast(
                    ToastKind::Error,
                    format!("git diff failed: {}", e.message()),
                );
            }
        }
        self.diff_selected = selected
            .and_then(|p| self.diff.iter().position(|f| f.path == p))
            .unwrap_or(0);
        if self.live == View::Diff {
            let keep = (self.cursor, self.top);
            let same = self.file.as_ref().map(|f| f.path.clone())
                == self.diff.get(self.diff_selected).map(|f| f.path.clone());
            self.open_diff();
            if same && self.lines().is_some() {
                let last = self.lines().map_or(0, |l| l.len() - 1);
                self.cursor = Cursor {
                    line: keep.0.line.min(last),
                    col: 0,
                };
                self.top = keep.1.min(last);
                self.scroll_to_cursor();
            }
        }
    }

    /// Show the selected file's diff in the (live, Diff view) code pane.
    pub(super) fn open_diff(&mut self) {
        self.cursor = Cursor::default();
        self.top = 0;
        self.left = 0;
        self.selection = None;
        let Some(f) = self.diff.get(self.diff_selected) else {
            self.file = None;
            self.marks.clear();
            return;
        };
        let content = if f.binary {
            Content::Placeholder("Binary file changed".to_string())
        } else {
            Content::Text(f.rows.iter().map(|r| r.text.clone()).collect())
        };
        let file = OpenFile {
            path: f.path.clone(),
            content,
            mtime: None,
            diff: (!f.binary).then(|| f.rows.clone()),
        };
        // The diff may have changed: highlight it afresh.
        self.highlights.remove(&file.key());
        self.file = Some(file);
        // Start on the first changed line; scroll only if it's off screen.
        let first = f
            .rows
            .iter()
            .position(|r| r.kind != git::RowKind::Hunk && r.kind != git::RowKind::Context);
        if let Some(i) = first {
            self.cursor.line = i;
            let rows = self.viewport().0;
            if i >= rows {
                self.top = i - rows / 3;
            }
        }
        self.refresh_marks();
    }

    pub(super) fn diff_move(&mut self, step: isize) {
        if self.diff.is_empty() {
            return;
        }
        let to =
            (self.diff_selected as isize + step).clamp(0, self.diff.len() as isize - 1) as usize;
        if to != self.diff_selected || self.file.is_none() {
            self.diff_selected = to;
            self.open_diff();
        }
    }

    /// The changed-files list.
    pub fn diff_list_props(&self) -> CodeProps {
        let focused = self.active_id() == Id::DiffFiles;
        let against = self.base.as_deref().unwrap_or("HEAD");
        let title = format!("Changes vs {against}");
        if self.diff.is_empty() {
            return CodeProps {
                focused,
                ..CodeProps::message(&title, "No changes")
            };
        }
        let rows = self
            .diff
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let line = Line::from(vec![
                    Span::styled(format!("+{} ", f.added), Style::new().fg(Color::Green)),
                    Span::styled(format!("-{} ", f.removed), Style::new().fg(Color::Red)),
                    Span::raw(f.path.display().to_string()),
                ]);
                match (i == self.diff_selected, focused) {
                    (true, true) => line.style(Style::new().add_modifier(Modifier::REVERSED)),
                    (true, false) => line.style(Style::new().add_modifier(Modifier::BOLD)),
                    _ => line,
                }
            })
            .collect();
        let height = super::layout(tuirealm::ratatui::layout::Rect::new(
            0,
            0,
            self.size.0,
            self.size.1,
        ))
        .tree
        .height
        .saturating_sub(2) as usize;
        CodeProps {
            title,
            rows,
            focused,
            scroll: self.diff_selected.saturating_sub(height.saturating_sub(1)) as u16,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{Model, Motion, Msg, Popup, SelectionKind, ToastKind};
    use crate::highlight::load_theme;
    use crate::model::{Span, View};
    use crate::store::MemStore;
    use crate::testutil::fixture_repo;
    use std::fs;
    use std::path::Path;

    fn model(root: &Path, base: Option<&str>) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(root.into(), theme, Box::new(MemStore::default()))
            .unwrap()
            .with_git(base.map(String::from));
        m.update(Msg::Resize(80, 20));
        m
    }

    fn press(m: &mut Model, msgs: &[Msg]) {
        msgs.iter().for_each(|msg| m.update(msg.clone()));
    }

    /// The milestone's "done when": the fixture's uncommitted change shows up
    /// and can be annotated.
    #[test]
    fn uncommitted_change_can_be_annotated() {
        let repo = fixture_repo();
        let mut m = model(repo.path(), None);
        assert_eq!(
            m.view,
            View::Diff,
            "starts in the Diff view when there are changes"
        );
        assert_eq!(m.diff.len(), 1);
        press(&mut m, &[Msg::DiffMove(0), Msg::ToggleFocus]);
        // Rows: @@, 3 context, blank added (new 4), VERSION added (new 5).
        assert_eq!(m.cursor.line, 4, "starts on the first change");
        assert_eq!(m.location().unwrap(), "web/app.ts 4:1");
        m.update(Msg::NewSession("Diff".into()));
        press(
            &mut m,
            &[
                Msg::ToggleSelection(SelectionKind::Line),
                Msg::Move(Motion::Down),
                Msg::AddNote,
            ],
        );
        let draft = m.draft.clone().unwrap();
        assert_eq!(draft.file, Path::new("web/app.ts"));
        assert_eq!(draft.span, Span::lines(4, 5));
        assert_eq!(draft.anchor.selected, "\nexport const VERSION = 2;");
        m.update(Msg::SaveNote("bump this from the build".into()));
        let note = &m.active_session().unwrap().notes[0];
        assert_eq!(note.span, Span::lines(4, 5));
        assert_eq!(m.marks.len(), 1);
        assert_eq!(
            (m.marks[0].start, m.marks[0].end),
            (4, 5),
            "marked on the diff rows"
        );
        // The same note shows in the Files view at the new-side lines.
        m.update(Msg::ShowView(View::Files));
        m.update(Msg::OpenFile("web/app.ts".into()));
        assert_eq!((m.marks[0].start, m.marks[0].end), (3, 4));
    }

    #[test]
    fn removed_lines_and_hunk_headers_cant_be_noted() {
        let repo = fixture_repo();
        let root = repo.path();
        fs::write(root.join("src/matcher.rs"), "pub fn matches() {}\n").unwrap();
        let mut m = model(root, None);
        m.update(Msg::NewSession("S".into()));
        let i = m
            .diff
            .iter()
            .position(|f| f.path == Path::new("src/matcher.rs"))
            .unwrap();
        press(
            &mut m,
            &[
                Msg::DiffMove(i as isize),
                Msg::ToggleFocus,
                Msg::Move(Motion::Top),
            ],
        );
        m.update(Msg::AddNote);
        assert_eq!(m.popup, None, "hunk header");
        m.update(Msg::Move(Motion::Down));
        m.update(Msg::AddNote);
        assert_eq!(m.popup, None, "removed line");
        assert!(m.toast.as_ref().unwrap().text.contains("Removed lines"));
        assert_eq!(m.toast.as_ref().unwrap().kind, ToastKind::Warning);
        // The added line works.
        press(&mut m, &[Msg::Move(Motion::Bottom), Msg::AddNote]);
        assert_eq!(m.popup, Some(Popup::Note));
        assert_eq!(m.draft.as_ref().unwrap().span, Span::lines(1, 1));
    }

    #[test]
    fn panes_keep_their_own_place() {
        let repo = fixture_repo();
        let mut m = model(repo.path(), None);
        m.update(Msg::ShowView(View::Files));
        press(
            &mut m,
            &[
                Msg::OpenFile("src/matcher.rs".into()),
                Msg::Move(Motion::Bottom),
            ],
        );
        m.update(Msg::ShowView(View::Diff));
        assert_eq!(m.file.as_ref().unwrap().path, Path::new("web/app.ts"));
        m.update(Msg::ShowView(View::Session));
        m.update(Msg::ShowView(View::Files));
        assert_eq!(m.location().unwrap(), "src/matcher.rs 9:1");
    }

    #[test]
    fn refresh_and_base() {
        let repo = fixture_repo();
        let root = repo.path();
        let mut m = model(root, None);
        fs::write(root.join("scripts/new.py"), "x = 1\n").unwrap();
        m.update(Msg::RefreshDiff);
        assert_eq!(m.diff.len(), 2);
        assert_eq!(
            m.diff[m.diff_selected].path,
            Path::new("web/app.ts"),
            "stays on the same file"
        );

        let m = model(root, Some("no-such-branch"));
        assert_eq!(m.toast.as_ref().unwrap().kind, ToastKind::Error);
        let m = model(root, Some("HEAD"));
        assert_eq!(m.diff.len(), 2, "merge-base(HEAD, HEAD) is HEAD");
    }

    #[test]
    fn no_changes_starts_in_files_and_outside_git_no_diff() {
        let repo = fixture_repo();
        fs::write(
            repo.path().join("web/app.ts"),
            "export function count(items: string[]): number {\n  return items.length;\n}\n",
        )
        .unwrap();
        let m = model(repo.path(), None);
        assert_eq!(m.view, View::Files);

        let dir = tempfile::tempdir().unwrap();
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(dir.path().into(), theme, Box::new(MemStore::default())).unwrap();
        m.update(Msg::ShowView(View::Diff));
        assert_eq!(m.view, View::Files);
        assert!(
            m.toast
                .as_ref()
                .unwrap()
                .text
                .starts_with("Not a git repository")
        );
    }
}
