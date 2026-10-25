//! Model, Msg and update: all app state and every change to it.

mod diff;
mod notes;
mod run;
mod search;
mod sessions;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use tuirealm::ratatui::layout::{Constraint, Layout, Rect};
use tuirealm::ratatui::text::Line;
use two_face::re_exports::syntect::highlighting::Theme;

pub use notes::{Draft, Selection, SelectionKind};
pub use run::run;
pub use search::{Picker, PickerKind};

use crate::components::code_view::{self, CodeProps, Mark};
use crate::components::session_view::NoteRow;
use crate::git::{FileDiff, Row};
use crate::highlight::{Highlighted, TAB};
use crate::model::{Session, State, View};
use crate::search::files::read_lines;
use crate::store::{Store, StoreError};

/// Files bigger than this show a placeholder.
const TREE_WIDTH: u16 = 32;
/// Lines highlighted per frame, so a jump deep into a big file never blocks.
// ponytail: fixed count (~35ms on a fast machine); make it time-based if
// slow grammars stutter.
const HIGHLIGHT_BATCH: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Id {
    FileTree,
    CodeView,
    DiffFiles,
    StatusBar,
    Help,
    NameInput,
    SessionPicker,
    SessionView,
    Toast,
    NoteHeader,
    NoteText,
    Confirm,
    QueryInput,
    PickerResults,
    NoteViewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Popup {
    Help,
    NewSession,
    SessionPicker,
    Note,
    ConfirmDelete,
    /// First run: offer to ignore `.tackline/`.
    Gitignore,
    /// Enter on a noted line: the full note.
    ViewNote,
    /// Ctrl+P
    Finder,
    /// Ctrl+F
    Search,
    /// `/`
    FindInFile,
    /// `:`
    GotoLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    HalfPageDown,
    HalfPageUp,
    LineStart,
    LineEnd,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Msg {
    /// Path relative to the repo root.
    OpenFile(PathBuf),
    Move(Motion),
    /// First `g` waits for a second one: `gg` goes to the top.
    PressG,
    ToggleFocus,
    ShowView(View),
    OpenPopup(Popup),
    ClosePopup,
    /// Create a session with this name and make it active.
    NewSession(String),
    /// Make the session at this index (in load order) active.
    PickSession(usize),
    /// Shift+Tab: make the next session active (wrapping).
    NextSession,
    /// Session view: select the previous/next note.
    NoteUp,
    NoteDown,
    /// Session view: open the selected note in the Files view.
    JumpToNote,
    /// Compile the active session and copy it.
    CopyPrompt,
    DismissToast,
    /// Something outside `update` failed (e.g. the clipboard).
    Error(String),
    /// `v` / `V`.
    ToggleSelection(SelectionKind),
    /// Shift+arrow.
    ExtendSelection(SelectionKind, Motion),
    ClearSelection,
    /// `c`: note the selection (or the cursor line).
    AddNote,
    /// Note popup: change the target session.
    CycleSession(isize),
    SaveNote(String),
    /// Session view: `e`, `d` (asks), confirmed delete, `J` / `K`.
    EditNote,
    AskDeleteNote,
    /// Enter on a noted line: show the whole note.
    ViewNote,
    DeleteNote,
    /// Answer to a yes/no popup.
    Confirm(bool),
    MoveNote(isize),
    /// Diff view: select the previous/next changed file.
    DiffMove(isize),
    /// `r`: reload the diff.
    RefreshDiff,
    /// The search input changed / Enter was pressed in it.
    QueryChanged(String),
    QueryEnter(String),
    PickerMove(isize),
    /// `n` / `N` after `/`.
    FindNext,
    FindPrev,
    /// Left click / drag at a screen position.
    Click {
        col: u16,
        row: u16,
    },
    Drag {
        col: u16,
        row: u16,
    },
    /// `w`: soft wrap on/off.
    ToggleWrap,
    /// Mouse wheel at a screen position; `lines` > 0 scrolls down.
    Wheel {
        col: u16,
        row: u16,
        lines: isize,
    },
    /// Terminal size (width, height).
    Resize(u16, u16),
    /// A component changed only its own widget state.
    Redraw,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(Vec<String>),
    /// Binary, too large, or unreadable.
    Placeholder(String),
}

pub struct OpenFile {
    /// Relative to the repo root.
    pub path: PathBuf,
    pub content: Content,
    mtime: Option<SystemTime>,
    /// In the Diff view: one row per line of `content`.
    pub diff: Option<Vec<Row>>,
}

impl OpenFile {
    /// Highlight cache key: a file and its diff are highlighted separately.
    fn key(&self) -> (PathBuf, bool) {
        (self.path.clone(), self.diff.is_some())
    }
}

/// The code pane's state; the Files and Diff views each keep one.
#[derive(Default)]
struct Pane {
    file: Option<OpenFile>,
    cursor: Cursor,
    top: usize,
    left: usize,
    selection: Option<Selection>,
    marks: Vec<Mark>,
    focus: Option<Focus>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    /// 0-based.
    pub line: usize,
    /// 0-based, in chars.
    pub col: usize,
}

pub struct Model {
    pub root: PathBuf,
    pub view: View,
    pub focus: Focus,
    pub file: Option<OpenFile>,
    pub cursor: Cursor,
    /// First visible line.
    pub top: usize,
    /// First visible display column.
    pub left: usize,
    pub size: (u16, u16),
    pub popup: Option<Popup>,
    pub pending_g: bool,
    pub quit: bool,
    pub sessions: Vec<Session>,
    pub state: State,
    pub theme: Theme,
    pub toast: Option<Toast>,
    /// Text for the run loop to put on the clipboard.
    pub clipboard: Option<String>,
    /// The active session's notes, resolved against the files on disk.
    pub notes: Vec<NoteRow>,
    /// Selected note in the Session view.
    pub note_cursor: usize,
    pub selection: Option<Selection>,
    pub draft: Option<Draft>,
    /// Gutter marks for the open file.
    pub marks: Vec<Mark>,
    /// `c` with no sessions: open the note popup once one is named.
    note_after_new_session: bool,
    pub picker: Option<Picker>,
    /// `w`: soft-wrap long lines in the code pane.
    pub wrap: bool,
    /// Last `/` query.
    pub find: Option<String>,
    /// Inside a git repo: the Diff view works.
    pub git: bool,
    /// `--base`: diff against `merge-base(HEAD, base)` instead of `HEAD`.
    pub base: Option<String>,
    pub diff: Vec<FileDiff>,
    /// Selected file in the Diff view's list.
    pub diff_selected: usize,
    /// Which view the live code pane fields belong to (Files or Diff).
    live: View,
    /// The other view's code pane.
    stashed: Pane,
    store: Box<dyn Store>,
    /// Per-file highlight caches, dropped when the file's mtime changes.
    highlights: HashMap<(PathBuf, bool), (Option<SystemTime>, Highlighted)>,
}

/// Screen areas: tree, code pane, status bar.
pub struct Areas {
    pub tree: Rect,
    pub code: Rect,
    pub main: Rect,
    pub status: Rect,
}

pub fn layout(area: Rect) -> Areas {
    let [main, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let [tree, code] = Layout::horizontal([
        Constraint::Length(TREE_WIDTH.min(main.width / 2)),
        Constraint::Min(1),
    ])
    .areas(main);
    Areas {
        tree,
        code,
        main,
        status,
    }
}

impl Model {
    /// Loads sessions and state from `store`, and reopens the last file.
    pub fn new(root: PathBuf, theme: Theme, store: Box<dyn Store>) -> Result<Self, StoreError> {
        let mut model = Self {
            sessions: store.load_sessions()?,
            state: store.load_state()?,
            store,
            toast: None,
            clipboard: None,
            notes: Vec::new(),
            note_cursor: 0,
            selection: None,
            draft: None,
            marks: Vec::new(),
            note_after_new_session: false,
            picker: None,
            wrap: false,
            find: None,
            git: false,
            base: None,
            diff: Vec::new(),
            diff_selected: 0,
            live: View::Files,
            stashed: Pane::default(),
            root,
            view: View::Files,
            focus: Focus::Tree,
            file: None,
            cursor: Cursor::default(),
            top: 0,
            left: 0,
            size: (80, 24),
            popup: None,
            pending_g: false,
            quit: false,
            theme,
            highlights: HashMap::new(),
        };
        if let Some(path) = model.state.last_file.clone() {
            model.open(path);
            model.focus = Focus::Tree;
        }
        Ok(model)
    }

    pub fn active_session(&self) -> Option<&Session> {
        let id = self.state.active_session_id.as_ref()?;
        self.sessions.iter().find(|s| &s.id == id)
    }

    /// The component that receives keys.
    pub fn active_id(&self) -> Id {
        match (self.popup, self.view, self.focus) {
            (Some(Popup::Help), ..) => Id::Help,
            (Some(Popup::NewSession), ..) => Id::NameInput,
            (Some(Popup::SessionPicker), ..) => Id::SessionPicker,
            (Some(Popup::Note), ..) => Id::NoteText,
            (Some(Popup::ConfirmDelete | Popup::Gitignore), ..) => Id::Confirm,
            (Some(Popup::ViewNote), ..) => Id::NoteViewer,
            (Some(Popup::Finder | Popup::Search | Popup::FindInFile | Popup::GotoLine), ..) => {
                Id::QueryInput
            }
            (None, View::Files, Focus::Tree) => Id::FileTree,
            (None, View::Diff, Focus::Tree) => Id::DiffFiles,
            (None, View::Files | View::Diff, Focus::Code) => Id::CodeView,
            (None, View::Session, _) => Id::SessionView,
        }
    }

    /// Visible code rows and columns (inside the pane's border, after the gutter).
    fn viewport(&self) -> (usize, usize) {
        let code = layout(Rect::new(0, 0, self.size.0, self.size.1)).code;
        let gutter = match self.file.as_ref().and_then(|f| f.diff.as_ref()) {
            Some(rows) => code_view::diff_gutter_width(code_view::diff_digits(rows)),
            None => self.lines().map_or(0, |l| code_view::gutter_width(l.len())),
        };
        (
            code.height.saturating_sub(2).max(1) as usize,
            (code.width.saturating_sub(2) as usize)
                .saturating_sub(gutter)
                .max(1),
        )
    }

    fn lines(&self) -> Option<&[String]> {
        match &self.file.as_ref()?.content {
            Content::Text(lines) => Some(lines),
            Content::Placeholder(_) => None,
        }
    }

    pub fn update(&mut self, msg: Msg) {
        let pending_g = std::mem::take(&mut self.pending_g);
        match msg {
            Msg::OpenFile(path) => self.open(path),
            Msg::Move(motion) => self.move_cursor(motion),
            Msg::PressG if pending_g => self.move_cursor(Motion::Top),
            Msg::PressG => self.pending_g = true,
            Msg::ToggleFocus => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Code,
                    Focus::Code => Focus::Tree,
                }
            }
            Msg::ShowView(view) => self.show_view(view),
            Msg::OpenPopup(Popup::Finder) => self.open_picker(PickerKind::Files),
            Msg::OpenPopup(Popup::Search) => self.open_picker(PickerKind::Code),
            Msg::OpenPopup(Popup::FindInFile | Popup::GotoLine) if self.lines().is_none() => {}
            Msg::OpenPopup(popup) => self.popup = Some(popup),
            Msg::ClosePopup => {
                self.popup = None;
                self.draft = None;
                self.picker = None;
                self.note_after_new_session = false;
            }
            Msg::NewSession(name) => self.new_session(&name),
            Msg::PickSession(i) => self.pick_session(i),
            Msg::NextSession => self.next_session(),
            Msg::NoteUp => self.note_cursor = self.note_cursor.saturating_sub(1),
            Msg::NoteDown => {
                self.note_cursor = (self.note_cursor + 1).min(self.notes.len().saturating_sub(1))
            }
            Msg::JumpToNote => self.jump_to_note(),
            Msg::CopyPrompt => self.copy_prompt(),
            Msg::DismissToast => self.toast = None,
            Msg::Error(text) => self.toast(ToastKind::Error, text),
            Msg::ToggleSelection(kind) => self.toggle_selection(kind),
            Msg::ExtendSelection(kind, motion) => self.extend_selection(kind, motion),
            Msg::ClearSelection => self.selection = None,
            Msg::AddNote => self.add_note(),
            Msg::CycleSession(step) => self.cycle_session(step),
            Msg::SaveNote(text) => self.save_note(&text),
            Msg::EditNote => self.edit_note(),
            Msg::AskDeleteNote => self.ask_delete_note(),
            Msg::ViewNote => self.view_note(),
            Msg::DeleteNote => self.delete_note(),
            Msg::Confirm(yes) => match self.popup.take() {
                Some(Popup::ConfirmDelete) if yes => self.delete_note(),
                Some(Popup::Gitignore) => self.answer_gitignore(yes),
                _ => {}
            },
            Msg::MoveNote(step) => self.move_note(step),
            Msg::QueryChanged(query) => self.query_changed(query),
            Msg::QueryEnter(query) => self.query_enter(query),
            Msg::PickerMove(step) => self.picker_move(step),
            Msg::FindNext => self.find_next(true),
            Msg::Wheel { col, row, lines } => self.wheel(col, row, lines),
            Msg::Click { col, row } => self.click(col, row),
            Msg::Drag { col, row } => self.drag(col, row),
            Msg::ToggleWrap => {
                self.wrap = !self.wrap;
                self.scroll_to_cursor();
            }
            Msg::DiffMove(step) => self.diff_move(step),
            Msg::RefreshDiff if self.view == View::Diff => self.refresh_diff(),
            Msg::RefreshDiff => {}
            Msg::FindPrev => self.find_next(false),
            Msg::Resize(w, h) => {
                self.size = (w, h);
                self.scroll_to_cursor();
            }
            Msg::Redraw => {}
            Msg::Quit => self.quit = true,
        }
    }

    fn open(&mut self, path: PathBuf) {
        let full = self.root.join(&path);
        let mtime = fs::metadata(&full).ok().and_then(|m| m.modified().ok());
        // Errors show with the path as given (relative), like before.
        let content = match read_lines(&full) {
            Ok(lines) => Content::Text(lines),
            Err(e) => Content::Placeholder(
                e.replace(&full.display().to_string(), &path.display().to_string()),
            ),
        };
        if self.state.last_file.as_ref() != Some(&path) {
            self.state.last_file = Some(path.clone());
            self.save_state();
        }
        let key = (path.clone(), false);
        if self.highlights.get(&key).is_some_and(|(t, _)| *t != mtime) {
            self.highlights.remove(&key);
        }
        self.file = Some(OpenFile {
            path,
            content,
            mtime,
            diff: None,
        });
        self.cursor = Cursor::default();
        self.top = 0;
        self.left = 0;
        self.focus = Focus::Code;
        self.selection = None;
        self.refresh_marks();
    }

    fn show_view(&mut self, view: View) {
        if view == View::Diff && !self.git {
            return self.toast(ToastKind::Warning, "Not a git repository: no Diff view");
        }
        if matches!(view, View::Files | View::Diff) {
            self.use_pane(view);
        }
        if view == View::Diff && self.file.is_none() {
            self.open_diff();
        }
        self.view = view;
        if view == View::Session {
            self.resolve_notes();
        }
        if self.state.last_view != Some(view) {
            self.state.last_view = Some(view);
            self.save_state();
        }
    }

    /// Make `view`'s code pane the live one (Files and Diff each keep theirs).
    fn use_pane(&mut self, view: View) {
        if view == self.live {
            return;
        }
        let s = &mut self.stashed;
        std::mem::swap(&mut self.file, &mut s.file);
        std::mem::swap(&mut self.cursor, &mut s.cursor);
        std::mem::swap(&mut self.top, &mut s.top);
        std::mem::swap(&mut self.left, &mut s.left);
        std::mem::swap(&mut self.selection, &mut s.selection);
        std::mem::swap(&mut self.marks, &mut s.marks);
        let focus = s.focus.replace(self.focus);
        self.focus = focus.unwrap_or(Focus::Tree);
        self.live = view;
        // Notes may have changed while the pane was away.
        self.refresh_marks();
    }

    fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        self.toast = Some(Toast {
            text: text.into(),
            kind,
            at: Instant::now(),
        });
    }

    /// Autosave; a failure shows up as an error toast.
    fn saved(&mut self, result: Result<(), StoreError>) {
        if let Err(e) = result {
            self.toast(ToastKind::Error, format!("Couldn't save: {e}"));
        }
    }

    fn save_state(&mut self) {
        let result = self.store.save_state(&self.state);
        self.saved(result);
    }

    fn wheel(&mut self, col: u16, row: u16, lines: isize) {
        let areas = layout(Rect::new(0, 0, self.size.0, self.size.1));
        let at = tuirealm::ratatui::layout::Position::new(col, row);
        match self.view {
            View::Files | View::Diff if areas.code.contains(at) => self.scroll_lines(lines),
            View::Diff if areas.tree.contains(at) => self.diff_move(lines.signum()),
            View::Session if areas.main.contains(at) && lines > 0 => self.update(Msg::NoteDown),
            View::Session if areas.main.contains(at) => self.update(Msg::NoteUp),
            _ => {}
        }
    }

    /// The (line, char column) under a screen position in the code pane.
    fn cell_at(&self, col: u16, row: u16) -> Option<(usize, usize)> {
        let lines = self.lines()?;
        let code = layout(Rect::new(0, 0, self.size.0, self.size.1)).code;
        let (rows, cols) = self.viewport();
        let gutter = (code.width.saturating_sub(2) as usize).saturating_sub(cols);
        let r = row.checked_sub(code.y + 1)? as usize;
        let screen = screen_rows(lines, self.top, self.left, rows, cols, self.wrap);
        let &(line, from) = screen.get(r.min(screen.len().checked_sub(1)?))?;
        let x = (col as usize).saturating_sub(code.x as usize + 1 + gutter);
        Some((line, char_at(&lines[line], from + x)))
    }

    fn click(&mut self, col: u16, row: u16) {
        let areas = layout(Rect::new(0, 0, self.size.0, self.size.1));
        let at = tuirealm::ratatui::layout::Position::new(col, row);
        match self.view {
            View::Files | View::Diff if areas.code.contains(at) => {
                self.focus = Focus::Code;
                if let Some((line, col)) = self.cell_at(col, row) {
                    self.cursor = Cursor { line, col };
                    self.selection = None;
                }
            }
            View::Diff if areas.tree.contains(at) => {
                self.focus = Focus::Tree;
                let scroll = self.diff_list_props().scroll as usize;
                if let Some(r) = row.checked_sub(areas.tree.y + 1) {
                    let i = r as usize + scroll;
                    if i < self.diff.len() {
                        self.diff_selected = i;
                        self.open_diff();
                    }
                }
            }
            View::Files if areas.tree.contains(at) => self.focus = Focus::Tree,
            _ => {}
        }
    }

    /// Drag in the code pane: a charwise selection from where it started.
    fn drag(&mut self, col: u16, row: u16) {
        if !matches!(self.view, View::Files | View::Diff) || self.focus != Focus::Code {
            return;
        }
        let Some((line, col)) = self.cell_at(col, row) else {
            return;
        };
        if self.selection.is_none() {
            self.selection = Some(Selection {
                kind: SelectionKind::Char,
                anchor: self.cursor,
            });
        }
        self.cursor = Cursor { line, col };
        self.scroll_to_cursor();
    }

    /// Scroll the view without moving the cursor, except to keep it on screen.
    fn scroll_lines(&mut self, lines: isize) {
        let Some(text) = self.lines() else { return };
        let rows = self.viewport().0;
        let len = text.len();
        let top = (self.top as isize + lines).clamp(0, len.saturating_sub(rows) as isize) as usize;
        let line = self.cursor.line.clamp(top, (top + rows - 1).min(len - 1));
        let col = self
            .cursor
            .col
            .min(text[line].chars().count().saturating_sub(1));
        self.top = top;
        self.cursor = Cursor { line, col };
    }

    fn move_cursor(&mut self, motion: Motion) {
        let Some(lines) = self.lines() else { return };
        let last = lines.len() - 1;
        let half = (self.viewport().0 / 2).max(1);
        let Cursor { mut line, mut col } = self.cursor;
        match motion {
            Motion::Up => line = line.saturating_sub(1),
            Motion::Down => line = (line + 1).min(last),
            Motion::Top => line = 0,
            Motion::Bottom => line = last,
            Motion::HalfPageDown => line = (line + half).min(last),
            Motion::HalfPageUp => line = line.saturating_sub(half),
            Motion::Left => col = col.saturating_sub(1),
            Motion::Right => col += 1,
            Motion::LineStart => col = 0,
            Motion::LineEnd => col = usize::MAX,
        }
        let len = lines[line].chars().count();
        self.cursor = Cursor {
            line,
            col: col.min(len.saturating_sub(1)),
        };
        self.scroll_to_cursor();
    }

    /// Keep the cursor inside the viewport.
    fn scroll_to_cursor(&mut self) {
        let Some(lines) = self.lines() else { return };
        let (rows, cols) = self.viewport();
        let x = display_col(&lines[self.cursor.line], self.cursor.col);
        let line = self.cursor.line;
        if !self.wrap {
            self.top = self.top.clamp(line.saturating_sub(rows - 1), line);
            self.left = self.left.clamp((x + 1).saturating_sub(cols), x);
            return;
        }
        // Wrapped: lines take several rows; scroll until the cursor's row fits.
        let mut top = self.top.min(line);
        let height = |l: usize| wrapped_height(&lines[l], cols);
        while top < line && (top..line).map(height).sum::<usize>() + x / cols >= rows {
            top += 1;
        }
        self.top = top;
        self.left = 0;
    }

    /// Everything the code pane shows; highlights the visible lines on demand.
    pub fn code_props(&mut self) -> CodeProps {
        let focused = self.active_id() == Id::CodeView;
        let (rows, cols) = self.viewport();
        let Some(file) = &self.file else {
            return CodeProps::message("No file", "Select a file in the tree and press Enter");
        };
        let title = file.path.display().to_string();
        let lines = match &file.content {
            Content::Placeholder(text) => return CodeProps::message(&title, text),
            Content::Text(lines) => lines,
        };
        let (_, hl) = self.highlights.entry(file.key()).or_insert_with(|| {
            (
                file.mtime,
                Highlighted::new(&file.path, &lines[0], &self.theme),
            )
        });
        let screen = screen_rows(lines, self.top, self.left, rows, cols, self.wrap);
        let end = screen.last().map_or(self.top, |(l, _)| l + 1);
        hl.ensure(
            lines,
            end.min(hl.lines.len() + HIGHLIGHT_BATCH),
            &self.theme,
        );
        // Rows syntect hasn't reached yet show as plain text for now.
        let visible: Vec<Line<'static>> = (self.top..end)
            .map(|i| match hl.lines.get(i) {
                Some(line) => line.clone(),
                None => Line::raw(lines[i].replace('\t', TAB)),
            })
            .collect();
        code_view::build(code_view::Input {
            title,
            highlighted: &visible,
            first_line: self.top,
            total_lines: lines.len(),
            rows: &screen,
            wrap: self.wrap,
            width: cols,
            cursor: focused
                .then_some(self.cursor.line)
                .map(|l| (l, display_col(&lines[l], self.cursor.col))),
            current_line: self.cursor.line,
            diff: file.diff.as_ref().map(|d| &d[self.top..end]),
            diff_digits: file.diff.as_deref().map_or(0, code_view::diff_digits),
            marks: &self.marks,
            selection: self.selection_view(),
            // The bottom border has room for the pane width minus its corners
            // and the " ● " prefix.
            footer: self
                .note_excerpt(cols + code_view::gutter_width(lines.len()).saturating_sub(5)),
            theme: &self.theme,
            focused,
        })
    }

    /// Visible rows are still waiting for highlighting; draw again soon.
    pub fn highlight_pending(&self) -> bool {
        let (Some(file), Some(lines)) = (&self.file, self.lines()) else {
            return false;
        };
        let end = (self.top + self.viewport().0).min(lines.len());
        self.highlights
            .get(&file.key())
            .is_some_and(|(_, hl)| hl.lines.len() < end)
    }

    /// `src/main.rs 12:4`, 1-based.
    pub fn location(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        if self.lines().is_none() {
            return Some(file.path.display().to_string());
        }
        if let Some(rows) = &file.diff {
            let row = rows.get(self.cursor.line)?;
            return Some(match (row.new, row.old) {
                (Some(n), _) => format!("{} {n}:{}", file.path.display(), self.cursor.col + 1),
                (None, Some(o)) => format!("{} -{o} (removed)", file.path.display()),
                (None, None) => file.path.display().to_string(),
            });
        }
        Some(format!(
            "{} {}:{}",
            file.path.display(),
            self.cursor.line + 1,
            self.cursor.col + 1
        ))
    }
}

/// Screen rows of the code pane as (line, first display column): one per
/// line from `left`, or with `wrap`, one per `cols`-wide piece of each line.
pub fn screen_rows(
    lines: &[String],
    top: usize,
    left: usize,
    rows: usize,
    cols: usize,
    wrap: bool,
) -> Vec<(usize, usize)> {
    let lines_shown = (top..lines.len()).take(rows);
    if !wrap {
        return lines_shown.map(|l| (l, left)).collect();
    }
    lines_shown
        .flat_map(|l| (0..wrapped_height(&lines[l], cols)).map(move |k| (l, k * cols)))
        .take(rows)
        .collect()
}

/// Rows a line takes when wrapped at `cols`.
fn wrapped_height(line: &str, cols: usize) -> usize {
    display_col(line, usize::MAX).div_ceil(cols).max(1)
}

/// The char at display column `x` in `line` (the last char past the end).
fn char_at(line: &str, x: usize) -> usize {
    let mut width = 0;
    for (i, c) in line.chars().enumerate() {
        width += if c == '\t' { TAB.len() } else { 1 };
        if width > x {
            return i;
        }
    }
    line.chars().count().saturating_sub(1)
}

/// Display column of char `col` in `line`, with tabs as `TAB`.
// ponytail: counts every other char as one column; wide CJK/emoji will drift.
pub fn display_col(line: &str, col: usize) -> usize {
    line.chars()
        .take(col)
        .map(|c| if c == '\t' { TAB.len() } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::load_theme;
    use crate::testutil::fixture_repo;

    fn model(root: &std::path::Path) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let store = Box::new(crate::store::MemStore::default());
        let mut m = Model::new(root.to_path_buf(), theme, store).unwrap();
        m.update(Msg::Resize(80, 12));
        m
    }

    fn press(m: &mut Model, msgs: &[Msg]) {
        msgs.iter().for_each(|msg| m.update(msg.clone()));
    }

    #[test]
    fn open_file_and_move() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        assert_eq!(m.active_id(), Id::FileTree);
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        assert_eq!(m.active_id(), Id::CodeView);
        assert_eq!(m.location().unwrap(), "src/matcher.rs 1:1");

        press(&mut m, &[Msg::Move(Motion::Down), Msg::Move(Motion::Down)]);
        assert_eq!(m.cursor, Cursor { line: 2, col: 0 });
        press(&mut m, &[Msg::Move(Motion::LineEnd)]);
        assert_eq!(m.cursor.col, "    for item in items {".len() - 1);
        // Moving to a shorter line clamps the column.
        press(&mut m, &[Msg::Move(Motion::Bottom)]);
        assert_eq!(m.cursor, Cursor { line: 8, col: 0 });
        press(&mut m, &[Msg::PressG, Msg::PressG]);
        assert_eq!(m.cursor.line, 0);
        // A lone `g` followed by something else doesn't jump.
        press(
            &mut m,
            &[
                Msg::Move(Motion::Bottom),
                Msg::PressG,
                Msg::Move(Motion::Up),
                Msg::PressG,
            ],
        );
        assert_eq!(m.cursor.line, 7);
    }

    #[test]
    fn scrolling_follows_cursor() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        // 12 rows: 1 status bar, 2 borders → 9 visible code lines.
        let body: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        fs::write(repo.path().join("long.txt"), body).unwrap();
        m.update(Msg::OpenFile("long.txt".into()));
        m.update(Msg::Move(Motion::HalfPageDown));
        assert_eq!((m.cursor.line, m.top), (4, 0));
        press(&mut m, &vec![Msg::Move(Motion::HalfPageDown); 3]);
        assert_eq!((m.cursor.line, m.top), (16, 8));
        m.update(Msg::Move(Motion::Bottom));
        assert_eq!((m.cursor.line, m.top), (99, 91));
        m.update(Msg::Move(Motion::HalfPageUp));
        assert_eq!((m.cursor.line, m.top), (95, 91));
        // Only what's been on screen got highlighted.
        let props = m.code_props();
        assert_eq!(props.rows.len(), 9);
        assert_eq!(
            m.highlights[&(PathBuf::from("long.txt"), false)]
                .1
                .lines
                .len(),
            100
        );
    }

    #[test]
    fn deep_jump_renders_plain_then_catches_up() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        let body: String = (1..=2000).map(|i| format!("let x{i} = {i};\n")).collect();
        fs::write(repo.path().join("big.rs"), body).unwrap();
        m.update(Msg::OpenFile("big.rs".into()));
        m.update(Msg::Move(Motion::Bottom));
        let props = m.code_props();
        assert_eq!(props.rows.len(), 9);
        assert!(m.highlight_pending());
        let mut frames = 1;
        while m.highlight_pending() {
            m.code_props();
            frames += 1;
        }
        assert_eq!(frames, 4);
    }

    #[test]
    fn wheel_scrolls_the_view_and_drags_the_cursor_along() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        let body: String = (1..=100).map(|i| format!("line {i}\n")).collect();
        fs::write(repo.path().join("long.txt"), body).unwrap();
        m.update(Msg::OpenFile("long.txt".into()));
        m.update(Msg::Move(Motion::Down));
        // 80x12: the code pane starts at x=32; 9 visible rows.
        let wheel = |lines| Msg::Wheel {
            col: 50,
            row: 5,
            lines,
        };
        m.update(wheel(3));
        assert_eq!((m.top, m.cursor.line), (3, 3), "cursor kept on screen");
        m.update(wheel(-3));
        assert_eq!(
            (m.top, m.cursor.line),
            (0, 3),
            "cursor stays put while visible"
        );
        for _ in 0..50 {
            m.update(wheel(3));
        }
        assert_eq!((m.top, m.cursor.line), (91, 91), "stops at the end");
        // Over the tree: nothing.
        m.update(Msg::Wheel {
            col: 5,
            row: 5,
            lines: -3,
        });
        assert_eq!(m.top, 91);
    }

    #[test]
    fn wrap_shows_long_lines_on_several_rows() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        // 60x12: tree 30, code pane 30 → 28 inside, gutter 4 → 24 columns.
        m.update(Msg::Resize(60, 12));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::ToggleWrap);
        let rows: Vec<String> = m
            .code_props()
            .rows
            .iter()
            .map(|r| r.to_string().trim_end().to_string())
            .collect();
        assert_eq!(rows[0], "1   pub fn matches(query: &s");
        assert_eq!(rows[1], "    tr, items: &[String]) ->");
        assert_eq!(rows[2], "     Vec<String> {");
        assert_eq!(rows[3], "2       let mut out = Vec::n");
        // End of the long line: no horizontal scroll while wrapped.
        m.update(Msg::Move(Motion::LineEnd));
        assert_eq!((m.left, m.top), (0, 0));
        // The last line still fits: 9 lines take 11 rows, so the top moves 2.
        m.update(Msg::Move(Motion::Bottom));
        assert_eq!(m.top, 2);
    }

    #[test]
    fn click_and_drag() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::ToggleFocus);
        assert_eq!(m.focus, Focus::Tree);
        // Code pane at x=32: border 1 + gutter 4 → text starts at x=37, y=1.
        m.update(Msg::Click {
            col: 37 + 4,
            row: 1 + 2,
        });
        assert_eq!(
            (m.focus, m.cursor),
            (Focus::Code, Cursor { line: 2, col: 4 })
        );
        m.update(Msg::Drag {
            col: 37 + 7,
            row: 1 + 2,
        });
        m.update(Msg::Drag {
            col: 37 + 20,
            row: 1 + 2,
        });
        assert_eq!(m.selection.unwrap().anchor, Cursor { line: 2, col: 4 });
        assert_eq!(m.cursor, Cursor { line: 2, col: 20 });
        // Past the end of the line: the last char.
        m.update(Msg::Click {
            col: 79,
            row: 1 + 1,
        });
        assert_eq!((m.cursor, m.selection), (Cursor { line: 1, col: 28 }, None));
    }

    #[test]
    fn horizontal_scroll() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::Resize(60, 12));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::Move(Motion::LineEnd));
        // Width 60 - tree 30 - borders 2 - gutter 4 = 24 columns.
        let line = "pub fn matches(query: &str, items: &[String]) -> Vec<String> {";
        assert_eq!(m.left, line.len() - 24);
        m.update(Msg::Move(Motion::LineStart));
        assert_eq!(m.left, 0);
    }

    #[test]
    fn placeholders_for_binary_large_and_missing() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        fs::write(repo.path().join("bin.dat"), [0u8, 1, 2]).unwrap();
        m.update(Msg::OpenFile("bin.dat".into()));
        assert_eq!(
            m.file.as_ref().unwrap().content,
            Content::Placeholder("Binary file".into())
        );
        let big = fs::File::create(repo.path().join("big.txt")).unwrap();
        big.set_len(crate::search::files::MAX_FILE_BYTES + 1)
            .unwrap();
        m.update(Msg::OpenFile("big.txt".into()));
        assert!(
            matches!(&m.file.as_ref().unwrap().content, Content::Placeholder(t) if t.contains("too large"))
        );
        m.update(Msg::OpenFile("missing.rs".into()));
        assert!(
            matches!(&m.file.as_ref().unwrap().content, Content::Placeholder(t) if t.contains("Can't open"))
        );
        // Movement on a placeholder is a no-op, not a panic.
        m.update(Msg::Move(Motion::Down));
        assert_eq!(m.cursor, Cursor::default());
    }

    #[test]
    fn highlight_cache_drops_on_mtime_change() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        let path = repo.path().join("web/app.ts");
        m.update(Msg::OpenFile("web/app.ts".into()));
        m.code_props();
        let f = fs::File::options().write(true).open(&path).unwrap();
        f.set_modified(SystemTime::UNIX_EPOCH).unwrap();
        m.update(Msg::OpenFile("web/app.ts".into()));
        assert!(
            !m.highlights
                .contains_key(&(PathBuf::from("web/app.ts"), false))
        );
    }

    #[test]
    fn views_focus_help_quit() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        press(&mut m, &[Msg::ToggleFocus]);
        assert_eq!(m.active_id(), Id::CodeView);
        press(&mut m, &[Msg::ShowView(View::Session)]);
        assert_eq!(m.active_id(), Id::SessionView);
        press(&mut m, &[Msg::OpenPopup(Popup::Help)]);
        assert_eq!(m.active_id(), Id::Help);
        press(&mut m, &[Msg::ClosePopup, Msg::ShowView(View::Files)]);
        assert_eq!(m.active_id(), Id::CodeView);
        press(&mut m, &[Msg::Quit]);
        assert!(m.quit);
    }
}
