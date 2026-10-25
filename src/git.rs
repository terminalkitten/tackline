use std::path::{Path, PathBuf};

use git2::{DiffOptions, Patch, Repository};

/// Working-tree root of the git repo containing `path`, if any.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    Repository::discover(path)
        .ok()?
        .workdir()
        .map(Path::to_path_buf)
}

/// Whether git ignores the directory `dir` (relative to the repo root),
/// whether or not it exists yet.
pub fn is_ignored(root: &Path, dir: &str) -> bool {
    let Ok(repo) = Repository::open(root) else {
        return false;
    };
    // A trailing slash tells libgit2 it's a directory, so `dir/` patterns
    // match even before the directory exists.
    [dir.to_string(), format!("{dir}/")]
        .iter()
        .any(|p| repo.is_path_ignored(p).unwrap_or(false))
}

/// Append `entry` to the repo's `.gitignore`, creating it if needed.
pub fn add_to_gitignore(root: &Path, entry: &str) -> std::io::Result<()> {
    let path = root.join(".gitignore");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(entry);
    text.push('\n');
    std::fs::write(path, text)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Hunk,
    Context,
    Added,
    Removed,
}

/// One line of a unified diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub kind: RowKind,
    /// 1-based, in the base version.
    pub old: Option<u32>,
    /// 1-based, in the working tree; notes attach to these.
    pub new: Option<u32>,
    /// Line content without the `+`/`-`/` ` prefix, or the `@@` header.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// New path, relative to the repo root (old path for deletions).
    pub path: PathBuf,
    pub added: usize,
    pub removed: usize,
    /// Empty for binary files.
    pub rows: Vec<Row>,
    pub binary: bool,
}

/// Working tree + index + untracked files, against `HEAD`, or against
/// `merge-base(HEAD, base)` when `base` is given (a branch, tag or commit).
pub fn diff(root: &Path, base: Option<&str>) -> Result<Vec<FileDiff>, git2::Error> {
    let repo = Repository::open(root)?;
    let head = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let base_commit = match (base, &head) {
        (Some(rev), Some(head)) => {
            let other = repo.revparse_single(rev)?.peel_to_commit()?;
            Some(repo.find_commit(repo.merge_base(head.id(), other.id())?)?)
        }
        (Some(rev), None) => Some(repo.revparse_single(rev)?.peel_to_commit()?),
        (None, head) => head.clone(),
    };
    let tree = base_commit.map(|c| c.tree()).transpose()?;
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    let diff = repo.diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))?;

    let mut files = Vec::new();
    for i in 0..diff.deltas().len() {
        let Some(patch) = Patch::from_diff(&diff, i)? else {
            continue;
        };
        let delta = patch.delta();
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let (_, added, removed) = patch.line_stats()?;
        let mut rows = Vec::new();
        for h in 0..patch.num_hunks() {
            let (hunk, lines) = patch.hunk(h)?;
            rows.push(Row {
                kind: RowKind::Hunk,
                old: None,
                new: None,
                text: text(hunk.header()),
            });
            for l in 0..lines {
                let line = patch.line_in_hunk(h, l)?;
                let kind = match line.origin() {
                    '+' => RowKind::Added,
                    '-' => RowKind::Removed,
                    ' ' => RowKind::Context,
                    // "\ No newline at end of file" markers.
                    _ => continue,
                };
                rows.push(Row {
                    kind,
                    old: line.old_lineno(),
                    new: line.new_lineno(),
                    text: text(line.content()),
                });
            }
        }
        files.push(FileDiff {
            path,
            added,
            removed,
            binary: delta.flags().is_binary(),
            rows,
        });
    }
    Ok(files)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_end_matches(['\n', '\r'])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn finds_root_from_subdirectory() {
        let repo = crate::testutil::fixture_repo();
        let root = repo.path().canonicalize().unwrap();
        let found = repo_root(&repo.path().join("src")).unwrap();
        assert_eq!(found.canonicalize().unwrap(), root);
    }

    #[test]
    fn none_outside_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        // ponytail: assumes the system temp dir isn't inside a git repo.
        assert_eq!(repo_root(dir.path()), None);
    }

    #[test]
    fn gitignore_entry() {
        let repo = crate::testutil::fixture_repo();
        let dir = ".tackline";
        assert!(!is_ignored(repo.path(), dir));
        fs::write(repo.path().join(".gitignore"), "target").unwrap();
        add_to_gitignore(repo.path(), ".tackline/").unwrap();
        assert_eq!(
            fs::read_to_string(repo.path().join(".gitignore")).unwrap(),
            "target\n.tackline/\n"
        );
        assert!(is_ignored(repo.path(), dir));
    }

    #[test]
    fn uncommitted_change() {
        let repo = crate::testutil::fixture_repo();
        let files = diff(repo.path(), None).unwrap();
        assert_eq!(files.len(), 1);
        let f = &files[0];
        assert_eq!(
            (f.path.to_str().unwrap(), f.added, f.removed),
            ("web/app.ts", 2, 0)
        );
        let rows: Vec<_> = f
            .rows
            .iter()
            .map(|r| (r.kind, r.old, r.new, r.text.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                (RowKind::Hunk, None, None, "@@ -1,3 +1,5 @@"),
                (
                    RowKind::Context,
                    Some(1),
                    Some(1),
                    "export function count(items: string[]): number {"
                ),
                (RowKind::Context, Some(2), Some(2), "  return items.length;"),
                (RowKind::Context, Some(3), Some(3), "}"),
                (RowKind::Added, None, Some(4), ""),
                (RowKind::Added, None, Some(5), "export const VERSION = 2;"),
            ]
        );
    }

    #[test]
    fn untracked_deleted_and_base() {
        let repo = crate::testutil::fixture_repo();
        let root = repo.path();
        fs::create_dir(root.join("new")).unwrap();
        fs::write(root.join("new/file.rs"), "fn x() {}\n").unwrap();
        fs::remove_file(root.join("scripts/score.py")).unwrap();
        let files = diff(root, None).unwrap();
        let summary: Vec<_> = files
            .iter()
            .map(|f| (f.path.display().to_string(), f.added, f.removed))
            .collect();
        assert_eq!(
            summary,
            [
                ("new/file.rs".into(), 1, 0),
                ("scripts/score.py".into(), 0, 3),
                ("web/app.ts".into(), 2, 0),
            ]
        );

        // Commit everything; --base HEAD~1 still shows it, plain diff doesn't.
        let git = Repository::open(root).unwrap();
        let mut index = git.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.update_all(["*"], None).unwrap();
        index.write().unwrap();
        let tree = git.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@e.com").unwrap();
        let parent = git.head().unwrap().peel_to_commit().unwrap();
        git.commit(Some("HEAD"), &sig, &sig, "second", &tree, &[&parent])
            .unwrap();
        assert!(diff(root, None).unwrap().is_empty());
        assert_eq!(diff(root, Some("HEAD~1")).unwrap().len(), 3);
        assert!(diff(root, Some("no-such-branch")).is_err());
    }
}
