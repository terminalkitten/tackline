//! Relocating notes after the file under them changed.

use std::cmp::Reverse;
use std::ops::Range;

use std::path::Path;

use crate::model::{Anchor, Note, Span};

const CONTEXT_LINES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relocation {
    Kept,
    Moved(Span),
    Stale,
}

impl Relocation {
    /// The span to use from now on, or `None` when stale.
    pub fn span(self, old: Span) -> Option<Span> {
        match self {
            Relocation::Kept => Some(old),
            Relocation::Moved(span) => Some(span),
            Relocation::Stale => None,
        }
    }
}

/// Snapshot the text under `span` and its surroundings. `None` if the span is
/// outside the file.
pub fn capture(span: Span, text: &str) -> Option<Anchor> {
    let doc = Doc::new(text);
    let range = doc.range(span)?;
    let (before, after) = doc.context(span.start_line as usize, span.end_line as usize);
    Some(Anchor {
        selected: doc.text[range].to_string(),
        before: before.join("\n"),
        after: after.join("\n"),
    })
}

/// Find where `anchor` lives in `text` now (spec §5 "Anchors").
pub fn relocate(span: Span, anchor: &Anchor, text: &str) -> Relocation {
    let doc = Doc::new(text);
    if doc.range(span).map(|r| &doc.text[r]) == Some(anchor.selected.as_str()) {
        return Relocation::Kept;
    }
    if anchor.selected.is_empty() {
        return Relocation::Stale;
    }
    let linewise = span.cols().is_none();
    // ponytail: match_indices skips overlapping matches; fine for code blocks.
    doc.text
        .match_indices(anchor.selected.as_str())
        .map(|(start, sel)| start..start + sel.len())
        .filter(|r| !linewise || (doc.is_line_start(r.start) && doc.is_line_end(r.end)))
        .map(|r| doc.span_at(r, linewise))
        .min_by_key(|c| {
            let score = doc.context_score(c, anchor);
            (Reverse(score), c.start_line.abs_diff(span.start_line))
        })
        .map_or(Relocation::Stale, Relocation::Moved)
}

/// Where a note is in a file now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Located {
    /// The relocated span; the stored one when stale.
    pub span: Span,
    pub stale: bool,
    /// Index of the note in the list passed to [`locate`].
    pub note: usize,
}

/// Where each of `notes` on `file` is in `text` now.
pub fn locate(notes: &[Note], file: &Path, text: &str) -> Vec<Located> {
    notes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.file == file)
        .map(|(i, n)| {
            let relocated = relocate(n.span, &n.anchor, text).span(n.span);
            Located {
                span: relocated.unwrap_or(n.span),
                stale: relocated.is_none(),
                note: i,
            }
        })
        .collect()
}

/// Read the note's file under `root` and relocate the note in it: the file's
/// text (if readable) and the note's span now (`None` when stale).
pub fn locate_on_disk(root: &Path, note: &Note) -> (Option<String>, Option<Span>) {
    let text = std::fs::read_to_string(root.join(&note.file)).ok();
    let span = text
        .as_deref()
        .and_then(|t| relocate(note.span, &note.anchor, t).span(note.span));
    (text, span)
}

/// File text normalised to `\n` endings, without the final newline, plus the
/// byte offset where each line starts.
struct Doc {
    text: String,
    starts: Vec<usize>,
}

impl Doc {
    fn new(text: &str) -> Self {
        let mut text = text.replace("\r\n", "\n");
        if text.ends_with('\n') {
            text.pop();
        }
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { text, starts }
    }

    fn line(&self, i: usize) -> &str {
        let end = self.starts.get(i + 1).map_or(self.text.len(), |s| s - 1);
        &self.text[self.starts[i]..end]
    }

    /// Byte range of `span`, `None` if it's out of bounds.
    fn range(&self, span: Span) -> Option<Range<usize>> {
        let (s, e) = (span.start_line as usize, span.end_line as usize);
        if s == 0 || s > e || e > self.starts.len() {
            return None;
        }
        let (first, last) = (self.line(s - 1), self.line(e - 1));
        let (start, end) = match span.cols() {
            None => (0, last.len()),
            Some((sc, ec)) => {
                let byte = |line: &str, n: usize| {
                    line.char_indices().nth(n).map_or(line.len(), |(b, _)| b)
                };
                (
                    byte(first, (sc as usize).saturating_sub(1)),
                    byte(last, ec as usize),
                )
            }
        };
        let range = self.starts[s - 1] + start..self.starts[e - 1] + end;
        (range.start <= range.end).then_some(range)
    }

    /// Up to `CONTEXT_LINES` lines before and after lines `s..=e` (1-based).
    fn context(&self, s: usize, e: usize) -> (Vec<&str>, Vec<&str>) {
        let before = (s - 1).saturating_sub(CONTEXT_LINES)..s - 1;
        let after = e..(e + CONTEXT_LINES).min(self.starts.len());
        (
            before.map(|i| self.line(i)).collect(),
            after.map(|i| self.line(i)).collect(),
        )
    }

    fn is_line_start(&self, b: usize) -> bool {
        b == 0 || self.text.as_bytes()[b - 1] == b'\n'
    }

    fn is_line_end(&self, b: usize) -> bool {
        b == self.text.len() || self.text.as_bytes()[b] == b'\n'
    }

    fn span_at(&self, r: Range<usize>, linewise: bool) -> Span {
        let line = self.starts.partition_point(|&s| s <= r.start);
        let col = self.text[self.starts[line - 1]..r.start].chars().count() + 1;
        let selected = &self.text[r];
        let newlines = selected.matches('\n').count();
        let end_line = (line + newlines) as u32;
        if linewise {
            return Span::lines(line as u32, end_line);
        }
        let tail = selected.rsplit('\n').next().unwrap_or("").chars().count();
        let end_col = if newlines == 0 { col - 1 + tail } else { tail };
        Span::chars(line as u32, col as u32, end_line, end_col.max(1) as u32)
    }

    /// How many context lines around `span` equal the anchor's.
    fn context_score(&self, span: &Span, anchor: &Anchor) -> usize {
        let lines = |s: &str| {
            if s.is_empty() {
                vec![]
            } else {
                s.split('\n').map(str::to_string).collect::<Vec<_>>()
            }
        };
        let (before, after) = self.context(span.start_line as usize, span.end_line as usize);
        let hits_before = lines(&anchor.before)
            .iter()
            .rev()
            .zip(before.iter().rev())
            .filter(|(a, b)| a == b)
            .count();
        let hits_after = lines(&anchor.after)
            .iter()
            .zip(&after)
            .filter(|(a, b)| a == b)
            .count();
        hits_before + hits_after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "fn a() {\n    one();\n}\n\nfn b() {\n    two();\n}\n";

    fn anchored(span: Span, text: &str) -> (Span, Anchor) {
        (span, capture(span, text).unwrap())
    }

    #[test]
    fn locate_notes_on_a_file() {
        let note = |file: &str, span: Span, text: &str| {
            Note::new(file.into(), span, capture(span, text).unwrap(), "x")
        };
        let notes = [
            note("a.rs", Span::lines(5, 7), SRC),
            note("b.rs", Span::lines(1, 1), SRC),
            note("a.rs", Span::lines(2, 2), SRC),
        ];
        let edited = format!("// new\n{}", SRC.replace("    one();", "    uno();"));
        let found = locate(&notes, Path::new("a.rs"), &edited);
        assert_eq!(
            found,
            [
                Located {
                    span: Span::lines(6, 8),
                    stale: false,
                    note: 0
                },
                Located {
                    span: Span::lines(2, 2),
                    stale: true,
                    note: 2
                },
            ]
        );
    }

    #[test]
    fn capture_takes_selection_and_context() {
        let a = capture(Span::lines(5, 7), SRC).unwrap();
        assert_eq!(a.selected, "fn b() {\n    two();\n}");
        assert_eq!(a.before, "}\n");
        assert_eq!(a.after, "");
        let a = capture(Span::chars(2, 5, 2, 9), SRC).unwrap();
        assert_eq!(a.selected, "one()");
    }

    #[test]
    fn capture_out_of_bounds() {
        assert_eq!(capture(Span::lines(7, 9), SRC), None);
        assert_eq!(capture(Span::lines(0, 1), SRC), None);
    }

    #[test]
    fn unchanged_file_keeps_span() {
        let (span, anchor) = anchored(Span::lines(5, 7), SRC);
        assert_eq!(relocate(span, &anchor, SRC), Relocation::Kept);
    }

    #[test]
    fn crlf_and_trailing_newline_are_ignored() {
        let (span, anchor) = anchored(Span::lines(5, 7), SRC);
        assert_eq!(
            relocate(span, &anchor, &SRC.replace('\n', "\r\n")),
            Relocation::Kept
        );
    }

    #[test]
    fn lines_inserted_above_move_span() {
        let (span, anchor) = anchored(Span::lines(5, 7), SRC);
        let edited = format!("use x;\nuse y;\n\n{SRC}");
        assert_eq!(
            relocate(span, &anchor, &edited),
            Relocation::Moved(Span::lines(8, 10))
        );
    }

    #[test]
    fn selected_text_moved_elsewhere() {
        let (span, anchor) = anchored(Span::lines(1, 3), SRC);
        let edited = "fn b() {\n    two();\n}\n\nfn a() {\n    one();\n}\n";
        assert_eq!(
            relocate(span, &anchor, edited),
            Relocation::Moved(Span::lines(5, 7))
        );
    }

    #[test]
    fn selected_text_deleted_is_stale() {
        let (span, anchor) = anchored(Span::lines(5, 7), SRC);
        assert_eq!(
            relocate(span, &anchor, "fn a() {\n    one();\n}\n"),
            Relocation::Stale
        );
        // Partial line match doesn't count for linewise notes.
        let (span, anchor) = anchored(Span::lines(2, 2), SRC);
        assert_eq!(
            relocate(span, &anchor, "fn a() {\n    one(); two();\n}\n"),
            Relocation::Stale
        );
    }

    #[test]
    fn duplicate_matches_pick_best_context() {
        let text = "// alpha\nlet x = 1;\nfoo();\n// beta\nlet x = 1;\nbar();\n";
        let (span, anchor) = anchored(Span::lines(5, 5), text);
        // Text above shifts both copies; context says it's the one after `// beta`.
        let edited = format!("// new\n\n{text}");
        assert_eq!(
            relocate(span, &anchor, &edited),
            Relocation::Moved(Span::lines(7, 7))
        );
        // Swap the blocks: follows the `// beta` context (line 2), not the
        // closer copy (line 6).
        let swapped = "// beta\nlet x = 1;\nbar();\n// alpha\n\nlet x = 1;\nfoo();\n";
        assert_eq!(
            relocate(span, &anchor, swapped),
            Relocation::Moved(Span::lines(2, 2))
        );
    }

    #[test]
    fn duplicate_with_equal_context_picks_nearest() {
        let text = "x();\nx();\nx();\n";
        let (span, mut anchor) = anchored(Span::lines(3, 3), text);
        anchor.before.clear();
        anchor.after.clear();
        // Old line 3 no longer holds `x();`; copies at 1 and 4, 4 is closer.
        let edited = "x();\nz();\nw();\nx();\n";
        assert_eq!(
            relocate(span, &anchor, edited),
            Relocation::Moved(Span::lines(4, 4))
        );
    }

    #[test]
    fn charwise_span_moves_with_columns() {
        let (span, anchor) = anchored(Span::chars(6, 5, 6, 9), SRC);
        assert_eq!(anchor.selected, "two()");
        let edited = SRC.replace("    two();", "    let _ = two();");
        assert_eq!(
            relocate(span, &anchor, &edited),
            Relocation::Moved(Span::chars(6, 13, 6, 17))
        );
    }

    #[test]
    fn charwise_multiline_and_unicode() {
        let text = "é = 1\nfn ü() {\n    x\n}\n";
        let (span, anchor) = anchored(Span::chars(2, 4, 3, 5), text);
        assert_eq!(anchor.selected, "ü() {\n    x");
        let edited = format!("\n{text}");
        assert_eq!(
            relocate(span, &anchor, &edited),
            Relocation::Moved(Span::chars(3, 4, 4, 5))
        );
    }
}
