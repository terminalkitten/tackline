//! Walking the repo, respecting `.gitignore`.

use std::path::{Path, PathBuf};

/// Every file and directory under `root` (relative paths, `is_dir`), skipping
/// gitignored entries and `.git`. Hidden files are included.
pub fn walk(root: &Path) -> Vec<(PathBuf, bool)> {
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.depth() > 0)
        .filter_map(|e| {
            let is_dir = e.file_type().is_some_and(|t| t.is_dir());
            Some((e.path().strip_prefix(root).ok()?.to_path_buf(), is_dir))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
