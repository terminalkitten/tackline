use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: Timestamp,
    /// Order here is the order in the compiled prompt.
    pub notes: Vec<Note>,
}

impl Session {
    pub fn new(name: &str) -> Self {
        Self {
            id: ulid::Ulid::generate().to_string(),
            name: name.to_string(),
            created_at: Timestamp::now(),
            notes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    /// Relative to the repo root.
    pub file: PathBuf,
    pub span: Span,
    pub anchor: Anchor,
    /// The prompt instruction.
    pub text: String,
    pub created_at: Timestamp,
}

impl Note {
    pub fn new(file: PathBuf, span: Span, anchor: Anchor, text: &str) -> Self {
        Self {
            id: ulid::Ulid::generate().to_string(),
            file,
            span,
            anchor,
            text: text.to_string(),
            created_at: Timestamp::now(),
        }
    }
}

/// Lines are 1-based and inclusive. Columns, present for charwise
/// selections, are 1-based, inclusive and counted in chars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: Option<u32>,
    pub end_col: Option<u32>,
}

impl Span {
    pub fn lines(start_line: u32, end_line: u32) -> Self {
        Self {
            start_line,
            end_line,
            start_col: None,
            end_col: None,
        }
    }

    pub fn chars(start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Self {
        Self {
            start_line,
            end_line,
            start_col: Some(start_col),
            end_col: Some(end_col),
        }
    }

    /// `(start_col, end_col)` when the span is charwise.
    pub fn cols(&self) -> Option<(u32, u32)> {
        self.start_col.zip(self.end_col)
    }

    /// UI form: `L88-95`, `L88` or `L12:4-L12:30`.
    pub fn ui_label(&self) -> String {
        match self.cols() {
            Some((sc, ec)) => format!("L{}:{sc}-L{}:{ec}", self.start_line, self.end_line),
            None => format!("L{}", self.label()),
        }
    }

    /// `88-95`, `88` or `12:4-12:30`.
    pub fn label(&self) -> String {
        match self.cols() {
            Some((sc, ec)) => format!("{}:{sc}-{}:{ec}", self.start_line, self.end_line),
            None if self.start_line == self.end_line => self.start_line.to_string(),
            None => format!("{}-{}", self.start_line, self.end_line),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// Exact selected text.
    pub selected: String,
    /// Up to 2 lines before the selection.
    pub before: String,
    /// Up to 2 lines after the selection.
    pub after: String,
}

/// `1 note`, `3 notes`.
pub fn note_count(n: usize) -> String {
    if n == 1 {
        "1 note".into()
    } else {
        format!("{n} notes")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Files,
    Diff,
    Session,
}

/// Contents of `.tackline/state.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    pub active_session_id: Option<String>,
    pub last_view: Option<View>,
    pub last_file: Option<PathBuf>,
}
