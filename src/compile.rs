//! Session → one Markdown prompt (spec §9).

use std::fmt;
use std::fs;
use std::path::Path;

use crate::anchor::relocate;
use crate::highlight::fence_lang;
use crate::model::{Note, Session, Span};

const MAX_SNIPPET_LINES: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning(pub String);

impl fmt::Display for Warning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Notes are relocated in memory only; stale ones are skipped with a warning.
pub fn compile(session: &Session, root: &Path) -> (String, Vec<Warning>) {
    let mut warnings = Vec::new();
    if session.notes.is_empty() {
        warnings.push(Warning(format!(
            "session \"{}\" has no notes",
            session.name
        )));
        return (String::new(), warnings);
    }
    let mut items = Vec::new();
    for note in &session.notes {
        let location = format!("{}:{}", note.file.display(), note.span.label());
        let text = match fs::read_to_string(root.join(&note.file)) {
            Ok(text) => text,
            Err(e) => {
                warnings.push(Warning(format!("skipped note at {location}: {e}")));
                continue;
            }
        };
        match relocate(note.span, &note.anchor, &text).span(note.span) {
            Some(span) => items.push(item(items.len() + 1, note, span, &text)),
            None => warnings.push(Warning(format!(
                "skipped stale note at {location}: the annotated code is gone"
            ))),
        }
    }
    if items.is_empty() {
        return (String::new(), warnings);
    }
    (
        format!("# {}\n\n{}", session.name, items.join("\n")),
        warnings,
    )
}

fn item(n: usize, note: &Note, span: Span, text: &str) -> String {
    let marker = format!("{n}. ");
    let indent = " ".repeat(marker.len());
    // Charwise spans still show their full lines, for context.
    let mut snippet: Vec<&str> = text
        .lines()
        .skip(span.start_line as usize - 1)
        .take((span.end_line - span.start_line) as usize + 1)
        .collect();
    if snippet.len() > MAX_SNIPPET_LINES {
        snippet.truncate(MAX_SNIPPET_LINES);
        snippet.push("…");
    }
    let fence = fence_for(&snippet);
    let lang = fence_lang(&note.file, text.lines().next().unwrap_or(""));

    let mut out = format!("{marker}`{}:{}`\n", note.file.display(), span.label());
    let mut push = |line: &str| {
        if !line.is_empty() {
            out.push_str(&indent);
            out.push_str(line);
        }
        out.push('\n');
    };
    push(&format!("{fence}{lang}"));
    snippet.iter().for_each(|l| push(l));
    push(&fence);
    note.text.lines().for_each(push);
    out
}

/// A backtick fence longer than any backtick run in the snippet.
fn fence_for(lines: &[&str]) -> String {
    let longest = lines
        .iter()
        .flat_map(|l| l.split(|c| c != '`'))
        .map(str::len)
        .max()
        .unwrap_or(0);
    "`".repeat((longest + 1).max(3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::capture;
    use crate::store::{FsStore, Store};
    use crate::testutil::fixture_repo;
    use std::path::PathBuf;

    fn note(root: &Path, file: &str, span: Span, text: &str) -> Note {
        let src = fs::read_to_string(root.join(file)).unwrap();
        Note::new(
            PathBuf::from(file),
            span,
            capture(span, &src).unwrap(),
            text,
        )
    }

    fn render(session: &Session, root: &Path) -> String {
        let (out, warnings) = compile(session, root);
        let warnings: Vec<_> = warnings.iter().map(|w| format!("warning: {w}")).collect();
        format!("{out}---\n{}", warnings.join("\n"))
    }

    /// The milestone 1 check: a session saved to disk, reloaded, and compiled
    /// after the files under it were edited.
    #[test]
    fn fixture_session() {
        let repo = fixture_repo();
        let root = repo.path();
        let mut session = Session::new("Search speedup");
        session.notes = vec![
            note(
                root,
                "src/matcher.rs",
                Span::lines(3, 7),
                "replace the loop with an iterator chain",
            ),
            note(
                root,
                "scripts/score.py",
                Span::chars(2, 13, 2, 38),
                "use math.fsum here",
            ),
            note(
                root,
                "web/App.vue",
                Span::lines(1, 3),
                "show the match count\nnext to the title",
            ),
        ];
        let store = FsStore::new(root);
        store.save_session(&session).unwrap();

        // Edit the file under the first note: its span moves down by two lines.
        let path = root.join("src/matcher.rs");
        let src = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("use std::fmt;\n\n{src}")).unwrap();

        let loaded = store.load_sessions().unwrap().remove(0);
        insta::assert_snapshot!(render(&loaded, root));
    }

    #[test]
    fn long_snippets_are_truncated() {
        let repo = fixture_repo();
        let body: String = (1..=50).map(|i| format!("line_{i} = {i}\n")).collect();
        fs::write(repo.path().join("long.py"), body).unwrap();
        let mut session = Session::new("Long");
        session.notes = vec![note(
            repo.path(),
            "long.py",
            Span::lines(1, 50),
            "split this up",
        )];
        insta::assert_snapshot!(render(&session, repo.path()));
    }

    #[test]
    fn stale_notes_are_skipped() {
        let repo = fixture_repo();
        let root = repo.path();
        let mut session = Session::new("Stale");
        session.notes = vec![
            note(root, "src/matcher.rs", Span::lines(1, 1), "gone soon"),
            note(root, "web/app.ts", Span::lines(1, 1), "still here"),
        ];
        fs::write(root.join("src/matcher.rs"), "// rewritten\n").unwrap();
        insta::assert_snapshot!(render(&session, root));
    }

    #[test]
    fn empty_session() {
        let repo = fixture_repo();
        let (out, warnings) = compile(&Session::new("Nothing"), repo.path());
        assert_eq!(out, "");
        assert_eq!(
            warnings,
            [Warning("session \"Nothing\" has no notes".into())]
        );
    }

    #[test]
    fn fence_outgrows_backticks_in_snippet() {
        assert_eq!(fence_for(&["let s = \"`x`\";"]), "```");
        assert_eq!(fence_for(&["/// ```rust"]), "````");
    }
}
