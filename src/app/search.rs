//! Ctrl+P file finder, Ctrl+F repo search, `/` search in the open file.

use std::path::PathBuf;

use super::{Cursor, Focus, Model, Popup, ToastKind};
use crate::components::code_view::CodeProps;
use crate::components::finder_popup;
use crate::model::View;
use crate::search::files::{find, walk};
use crate::search::grep::{Hit, grep};

/// Results kept per search.
const LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerKind {
    Files,
    Code,
}

/// State of the Ctrl+P / Ctrl+F popup.
#[derive(Debug, Clone, PartialEq)]
pub struct Picker {
    pub kind: PickerKind,
    pub query: String,
    /// File finder hits are line 1, col 1.
    pub hits: Vec<Hit>,
    pub selected: usize,
    /// Every file, for the finder.
    files: Vec<String>,
    /// Query the current code hits are for.
    searched: Option<String>,
}

impl Model {
    pub(super) fn open_picker(&mut self, kind: PickerKind) {
        let files = match kind {
            PickerKind::Files => walk(&self.root)
                .into_iter()
                .filter(|(_, is_dir)| !is_dir)
                .map(|(p, _)| p.display().to_string())
                .collect(),
            PickerKind::Code => Vec::new(),
        };
        let mut picker = Picker {
            kind,
            query: String::new(),
            hits: Vec::new(),
            selected: 0,
            files,
            searched: None,
        };
        if kind == PickerKind::Files {
            picker.hits = file_hits(&find(&picker.files, "", LIMIT));
        }
        self.picker = Some(picker);
        self.popup = Some(match kind {
            PickerKind::Files => Popup::Finder,
            PickerKind::Code => Popup::Search,
        });
    }

    pub(super) fn query_changed(&mut self, query: String) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.kind == PickerKind::Files {
            picker.hits = file_hits(&find(&picker.files, &query, LIMIT));
            picker.selected = 0;
        }
        picker.query = query;
    }

    pub(super) fn query_enter(&mut self, query: String) {
        match self.popup {
            Some(Popup::FindInFile) => {
                self.popup = None;
                self.find = Some(query).filter(|q| !q.is_empty());
                self.find_next(true);
            }
            Some(Popup::Finder | Popup::Search) => self.picker_accept(query),
            _ => {}
        }
    }

    pub(super) fn picker_move(&mut self, step: isize) {
        if let Some(p) = self.picker.as_mut().filter(|p| !p.hits.is_empty()) {
            p.selected = (p.selected as isize + step).clamp(0, p.hits.len() as isize - 1) as usize;
        }
    }

    /// Enter: search (Ctrl+F with a new query), or open the selected hit.
    fn picker_accept(&mut self, query: String) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.kind == PickerKind::Code && picker.searched.as_ref() != Some(&query) {
            if query.is_empty() {
                return;
            }
            picker.hits = grep(&self.root, &query, LIMIT);
            picker.selected = 0;
            picker.searched = Some(query.clone());
            if picker.hits.is_empty() {
                self.toast(ToastKind::Warning, format!("No matches for \"{query}\""));
            }
            return;
        }
        let Some(hit) = picker.hits.get(picker.selected).cloned() else {
            return;
        };
        self.popup = None;
        self.picker = None;
        self.goto(hit.path, hit.line - 1, hit.col - 1);
    }

    /// Open `path` in the Files view at (line, col), 0-based, centered.
    pub(super) fn goto(&mut self, path: PathBuf, line: usize, col: usize) {
        self.open(path);
        self.view = View::Files;
        self.focus = Focus::Code;
        let Some(lines) = self.lines() else {
            return;
        };
        let line = line.min(lines.len() - 1);
        let col = col.min(lines[line].chars().count().saturating_sub(1));
        self.cursor = Cursor { line, col };
        self.top = line.saturating_sub(self.viewport().0 / 2);
        self.scroll_to_cursor();
    }

    /// `/` then `n` / `N`: next or previous match of `find`, wrapping.
    pub(super) fn find_next(&mut self, forward: bool) {
        let Some(query) = self.find.clone() else {
            return;
        };
        let Some(lines) = self.lines() else {
            return;
        };
        let matches = find_in_lines(lines, &query);
        let here = (self.cursor.line, self.cursor.col);
        let next = if forward {
            matches.iter().find(|m| **m > here).or(matches.first())
        } else {
            matches.iter().rev().find(|m| **m < here).or(matches.last())
        };
        match next.copied() {
            Some((line, col)) => {
                self.cursor = Cursor { line, col };
                let rows = self.viewport().0;
                if line < self.top || line >= self.top + rows {
                    self.top = line.saturating_sub(rows / 2);
                }
                self.scroll_to_cursor();
            }
            None => self.toast(ToastKind::Warning, format!("No match for \"{query}\"")),
        }
    }

    /// The Ctrl+P / Ctrl+F results list.
    pub fn picker_props(&self) -> Option<CodeProps> {
        let picker = self.picker.as_ref()?;
        let screen = tuirealm::ratatui::layout::Rect::new(0, 0, self.size.0, self.size.1);
        let height = finder_popup::areas(screen).1.height.saturating_sub(2) as usize;
        Some(finder_popup::results(picker, height))
    }
}

fn file_hits(files: &[String]) -> Vec<Hit> {
    files
        .iter()
        .map(|f| Hit {
            path: f.into(),
            line: 1,
            col: 1,
            text: String::new(),
        })
        .collect()
}

/// Every (line, col) where `query` starts, 0-based, smart case.
// ponytail: lowercasing can shift columns for the few chars whose lowercase
// has a different length (e.g. `İ`).
fn find_in_lines(lines: &[String], query: &str) -> Vec<(usize, usize)> {
    let sensitive = query.chars().any(char::is_uppercase);
    let query = if sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let line = if sensitive {
            line.clone()
        } else {
            line.to_lowercase()
        };
        out.extend(
            line.match_indices(&query)
                .map(|(b, _)| (n, line[..b].chars().count())),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Msg, Popup};
    use crate::highlight::load_theme;
    use crate::store::MemStore;
    use crate::testutil::fixture_repo;
    use std::path::Path;

    fn model(root: &Path) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(root.into(), theme, Box::new(MemStore::default())).unwrap();
        m.update(Msg::Resize(80, 20));
        m
    }

    #[test]
    fn finder_opens_the_picked_file() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::OpenPopup(Popup::Finder));
        assert_eq!(m.picker.as_ref().unwrap().hits.len(), 4);
        m.update(Msg::QueryChanged("app".into()));
        let hits: Vec<_> = m
            .picker
            .as_ref()
            .unwrap()
            .hits
            .iter()
            .map(|h| h.path.clone())
            .collect();
        assert_eq!(hits.len(), 2);
        m.update(Msg::PickerMove(1));
        m.update(Msg::PickerMove(5));
        assert_eq!(m.picker.as_ref().unwrap().selected, 1, "clamped");
        m.update(Msg::QueryEnter("app".into()));
        assert_eq!((m.popup, m.view), (None, View::Files));
        assert_eq!(m.file.as_ref().unwrap().path, hits[1]);
    }

    #[test]
    fn repo_search_jumps_to_the_hit() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::ShowView(View::Session));
        m.update(Msg::OpenPopup(Popup::Search));
        m.update(Msg::QueryChanged("items".into()));
        assert!(
            m.picker.as_ref().unwrap().hits.is_empty(),
            "searches on Enter only"
        );
        m.update(Msg::QueryEnter("items".into()));
        assert_eq!(m.picker.as_ref().unwrap().hits.len(), 4);
        assert_eq!(m.popup, Some(Popup::Search), "first Enter searches");
        m.update(Msg::PickerMove(1));
        m.update(Msg::QueryEnter("items".into()));
        assert_eq!((m.popup, m.view), (None, View::Files));
        assert_eq!(m.location().unwrap(), "src/matcher.rs 3:17");

        m.update(Msg::OpenPopup(Popup::Search));
        m.update(Msg::QueryEnter("nothing-like-this".into()));
        assert_eq!(m.toast.as_ref().unwrap().kind, ToastKind::Warning);
    }

    #[test]
    fn find_in_file_next_prev_wraps() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::OpenPopup(Popup::FindInFile));
        m.update(Msg::QueryEnter("item".into()));
        assert_eq!(m.popup, None);
        assert_eq!(m.location().unwrap(), "src/matcher.rs 1:29");
        m.update(Msg::FindNext);
        assert_eq!(m.location().unwrap(), "src/matcher.rs 3:9");
        m.update(Msg::FindPrev);
        m.update(Msg::FindPrev);
        assert_eq!(
            m.location().unwrap(),
            "src/matcher.rs 5:22",
            "wraps to the last match"
        );
        m.update(Msg::OpenPopup(Popup::FindInFile));
        m.update(Msg::QueryEnter("ITEM".into()));
        assert_eq!(m.toast.as_ref().unwrap().text, "No match for \"ITEM\"");
    }

    #[test]
    fn deep_search_hit_is_centered() {
        let repo = fixture_repo();
        let body: String = (1..=300).map(|i| format!("line {i}\n")).collect();
        std::fs::write(repo.path().join("long.txt"), body).unwrap();
        let mut m = model(repo.path());
        m.update(Msg::OpenPopup(Popup::Search));
        m.update(Msg::QueryEnter("line 250$".into()));
        m.update(Msg::QueryEnter("line 250$".into()));
        assert_eq!((m.cursor.line, m.top), (249, 249 - 8));
    }
}
