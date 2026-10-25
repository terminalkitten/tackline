//! Walking the repo, respecting `.gitignore`.

use std::path::{Path, PathBuf};

/// Every file and directory under `root` (relative paths, `is_dir`), skipping
/// gitignored entries, `.git` and `.tackline`. Hidden files are included.
pub fn walk(root: &Path) -> Vec<(PathBuf, bool)> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        // Git's and tackline's own data aren't part of the code.
        .filter_entry(|e| e.file_name() != ".git" && e.file_name() != crate::store::DATA_DIR)
        // Same order on every OS (directory listing order isn't).
        .sort_by_file_name(|a, b| a.cmp(b))
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.depth() > 0)
        .filter_map(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            Some((e.path().strip_prefix(root).ok()?.to_path_buf(), is_dir))
        })
        .collect()
}

/// Files bigger than this aren't shown.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// A file's lines for viewing (at least one), or why it can't be shown:
/// missing, too large, binary. Invalid UTF-8 is replaced, not refused.
pub fn read_lines(path: &Path) -> Result<Vec<String>, String> {
    let name = path.display();
    let meta = std::fs::metadata(path).map_err(|e| format!("Can't open {name}: {e}"))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!(
            "File too large to show ({} MB)",
            meta.len() / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| format!("Can't read {name}: {e}"))?;
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return Err("Binary file".to_string());
    }
    let mut lines: Vec<String> = String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    Ok(lines)
}

/// Files matching `query` fuzzily (nucleo, path-aware, smart case), best
/// first, at most `limit`. An empty query keeps the order of `files`.
pub fn find(files: &[String], query: &str, limit: usize) -> Vec<String> {
    use nucleo::pattern::{CaseMatching, Normalization, Pattern};
    let mut matcher = nucleo::Matcher::new(nucleo::Config::DEFAULT.match_paths());
    Pattern::parse(query, CaseMatching::Smart, Normalization::Smart)
        .match_list(files, &mut matcher)
        .into_iter()
        .take(limit)
        .map(|(file, _)| file.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_find() {
        let files: Vec<String> = [
            "src/matcher.rs",
            "scripts/score.py",
            "web/app.ts",
            "web/App.vue",
        ]
        .map(String::from)
        .into();
        assert_eq!(find(&files, "mtch", 10), ["src/matcher.rs"]);
        assert_eq!(find(&files, "app", 10).len(), 2);
        assert_eq!(find(&files, "App", 10), ["web/App.vue"], "smart case");
        assert_eq!(find(&files, "", 2), ["src/matcher.rs", "scripts/score.py"]);
    }

    #[test]
    fn walk_skips_tackline_data() {
        let repo = crate::testutil::fixture_repo();
        std::fs::create_dir_all(repo.path().join(".tackline/sessions")).unwrap();
        std::fs::write(repo.path().join(".tackline/sessions/x.json"), "{}").unwrap();
        assert!(
            walk(repo.path())
                .iter()
                .all(|(p, _)| !p.starts_with(".tackline"))
        );
    }

    #[test]
    fn walk_respects_gitignore() {
        let repo = crate::testutil::fixture_repo();
        std::fs::write(repo.path().join(".gitignore"), "ignored/\n").unwrap();
        std::fs::create_dir(repo.path().join("ignored")).unwrap();
        std::fs::write(repo.path().join("ignored/x.rs"), "").unwrap();
        let mut paths: Vec<_> = walk(repo.path())
            .into_iter()
            .map(|(p, d)| format!("{}{}", p.display(), if d { "/" } else { "" }))
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                ".gitignore",
                "scripts/",
                "scripts/score.py",
                "src/",
                "src/matcher.rs",
                "web/",
                "web/App.vue",
                "web/app.ts"
            ]
        );
    }
}
