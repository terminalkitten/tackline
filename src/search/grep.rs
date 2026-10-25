//! Repo-wide code search with ripgrep's libraries.

use std::path::{Path, PathBuf};

use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};

use super::files::walk;

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    /// Relative to the repo root.
    pub path: PathBuf,
    /// 1-based.
    pub line: usize,
    /// 1-based, in chars.
    pub col: usize,
    /// The matching line, trimmed.
    pub text: String,
}

/// Smart case; a query that isn't a valid regex is searched literally.
fn matcher(query: &str) -> RegexMatcher {
    let mut builder = RegexMatcherBuilder::new();
    builder.case_smart(true);
    builder
        .build(query)
        .or_else(|_| builder.fixed_strings(true).build(query))
        .expect("a literal pattern always builds")
}

/// Search every non-ignored file under `root`; stops after `limit` hits.
// ponytail: runs on the UI thread when Enter is pressed; move to a worker if
// huge repos make it stall.
pub fn grep(root: &Path, query: &str, limit: usize) -> Vec<Hit> {
    let matcher = matcher(query);
    let mut searcher = SearcherBuilder::new()
        .binary_detection(BinaryDetection::quit(0))
        .line_number(true)
        .build();
    let mut hits = Vec::new();
    for (path, is_dir) in walk(root) {
        if is_dir {
            continue;
        }
        let sink = UTF8(|line, text| {
            let col = matcher
                .find(text.as_bytes())?
                .map_or(1, |m| text[..m.start()].chars().count() + 1);
            hits.push(Hit {
                path: path.clone(),
                line: line as usize,
                col,
                text: text.trim().to_string(),
            });
            Ok(hits.len() < limit)
        });
        // Unreadable or non-UTF-8 files are skipped.
        let _ = searcher.search_path(&matcher, root.join(&path), sink);
        if hits.len() >= limit {
            break;
        }
    }
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_lines_with_columns() {
        let repo = crate::testutil::fixture_repo();
        let hits = grep(repo.path(), "items", 100);
        let found: Vec<_> = hits
            .iter()
            .map(|h| format!("{}:{}:{}", h.path.display(), h.line, h.col))
            .collect();
        assert_eq!(
            found,
            [
                "src/matcher.rs:1:29",
                "src/matcher.rs:3:17",
                "web/app.ts:1:23",
                "web/app.ts:2:10"
            ]
        );
        assert_eq!(hits[1].text, "for item in items {");
    }

    #[test]
    fn smart_case_literal_fallback_and_limit() {
        let repo = crate::testutil::fixture_repo();
        assert!(
            grep(repo.path(), "ITEMS", 100).is_empty(),
            "uppercase is case-sensitive"
        );
        assert_eq!(
            grep(repo.path(), "contains(", 100).len(),
            1,
            "invalid regex → literal"
        );
        assert_eq!(grep(repo.path(), "e", 3).len(), 3);
    }
}
