//! Model, Msg and update: all app state and every change to it.

mod run;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use tuirealm::ratatui::layout::{Constraint, Layout, Rect};
use tuirealm::ratatui::text::Line;
use two_face::re_exports::syntect::highlighting::Theme;

pub use run::run;

use crate::components::code_view::{self, CodeProps};
use crate::highlight::{Highlighted, TAB};
use crate::model::{Session, State, View};

/// Files bigger than this show a placeholder.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const TREE_WIDTH: u16 = 32;
/// Lines highlighted per frame, so a jump deep into a big file never blocks.
// ponytail: fixed count (~35ms on a fast machine); make it time-based if
// slow grammars stutter.
const HIGHLIGHT_BATCH: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Id {
    FileTree,
    CodeView,
    Placeholder,
    StatusBar,
    Help,
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
    ToggleHelp,
    CloseHelp,
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
    pub help: bool,
    pub pending_g: bool,
    pub quit: bool,
    pub sessions: Vec<Session>,
    pub state: State,
    pub theme: Theme,
    /// Per-file highlight caches, dropped when the file's mtime changes.
    highlights: HashMap<PathBuf, (Option<SystemTime>, Highlighted)>,
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
    pub fn new(root: PathBuf, theme: Theme, sessions: Vec<Session>, state: State) -> Self {
        Self {
            root,
            view: View::Files,
            focus: Focus::Tree,
            file: None,
            cursor: Cursor::default(),
            top: 0,
            left: 0,
            size: (80, 24),
            help: false,
            pending_g: false,
            quit: false,
            sessions,
            state,
            theme,
            highlights: HashMap::new(),
        }
    }

    pub fn active_session(&self) -> Option<&Session> {
        let id = self.state.active_session_id.as_ref()?;
        self.sessions.iter().find(|s| &s.id == id)
    }

    /// The component that receives keys.
    pub fn active_id(&self) -> Id {
        match (self.help, self.view, self.focus) {
            (true, ..) => Id::Help,
            (false, View::Files, Focus::Tree) => Id::FileTree,
            (false, View::Files, Focus::Code) => Id::CodeView,
            _ => Id::Placeholder,
        }
    }

    /// Visible code rows and columns (inside the pane's border, after the gutter).
    fn viewport(&self) -> (usize, usize) {
        let code = layout(Rect::new(0, 0, self.size.0, self.size.1)).code;
        let gutter = self.lines().map_or(0, |l| code_view::gutter_width(l.len()));
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
            Msg::ShowView(view) => self.view = view,
            Msg::ToggleHelp => self.help = !self.help,
            Msg::CloseHelp => self.help = false,
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
        let meta = fs::metadata(&full);
        let mtime = meta.as_ref().ok().and_then(|m| m.modified().ok());
        let content = match meta {
            Err(e) => Content::Placeholder(format!("Can't open {}: {e}", path.display())),
            Ok(m) if m.len() > MAX_FILE_BYTES => Content::Placeholder(format!(
                "File too large to show ({} MB)",
                m.len() / (1024 * 1024)
            )),
            Ok(_) => match fs::read(&full) {
                Err(e) => Content::Placeholder(format!("Can't read {}: {e}", path.display())),
                Ok(bytes) if bytes.iter().take(8000).any(|&b| b == 0) => {
                    Content::Placeholder("Binary file".to_string())
                }
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
                    if lines.is_empty() {
                        lines.push(String::new());
                    }
                    Content::Text(lines)
                }
            },
        };
        if self.highlights.get(&path).is_some_and(|(t, _)| *t != mtime) {
            self.highlights.remove(&path);
        }
        self.file = Some(OpenFile {
            path,
            content,
            mtime,
        });
        self.cursor = Cursor::default();
        self.top = 0;
        self.left = 0;
        self.focus = Focus::Code;
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
        self.top = self.top.clamp(line.saturating_sub(rows - 1), line);
        self.left = self.left.clamp((x + 1).saturating_sub(cols), x);
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
        let (_, hl) = self.highlights.entry(file.path.clone()).or_insert_with(|| {
            (
                file.mtime,
                Highlighted::new(&file.path, &lines[0], &self.theme),
            )
        });
        let end = (self.top + rows).min(lines.len());
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
            left: self.left,
            width: cols,
            cursor: Some(self.cursor.line)
                .filter(|_| focused)
                .map(|l| (l, display_col(&lines[l], self.cursor.col))),
            current_line: self.cursor.line,
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
            .get(&file.path)
            .is_some_and(|(_, hl)| hl.lines.len() < end)
    }

    /// `src/main.rs 12:4`, 1-based.
    pub fn location(&self) -> Option<String> {
        let file = self.file.as_ref()?;
        Some(format!(
            "{} {}:{}",
            file.path.display(),
            self.cursor.line + 1,
            self.cursor.col + 1
        ))
    }
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
        let mut m = Model::new(root.to_path_buf(), theme, vec![], State::default());
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
        assert_eq!(m.highlights[&PathBuf::from("long.txt")].1.lines.len(), 100);
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
        big.set_len(MAX_FILE_BYTES + 1).unwrap();
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
        assert!(!m.highlights.contains_key(&PathBuf::from("web/app.ts")));
    }

    #[test]
    fn views_focus_help_quit() {
        let repo = fixture_repo();
        let mut m = model(repo.path());
        press(&mut m, &[Msg::ToggleFocus]);
        assert_eq!(m.active_id(), Id::CodeView);
        press(&mut m, &[Msg::ShowView(View::Diff)]);
        assert_eq!(m.active_id(), Id::Placeholder);
        press(&mut m, &[Msg::ToggleHelp]);
        assert_eq!(m.active_id(), Id::Help);
        press(&mut m, &[Msg::CloseHelp, Msg::ShowView(View::Files)]);
        assert_eq!(m.active_id(), Id::CodeView);
        press(&mut m, &[Msg::Quit]);
        assert!(m.quit);
    }
}
