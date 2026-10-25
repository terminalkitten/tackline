use std::path::{Path, PathBuf};

/// Working-tree root of the git repo containing `path`, if any.
pub fn repo_root(path: &Path) -> Option<PathBuf> {
    git2::Repository::discover(path)
        .ok()?
        .workdir()
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
