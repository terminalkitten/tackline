//! Fixture git repo shared by tests.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

const FILES: &[(&str, &str)] = &[
    (
        "src/matcher.rs",
        "pub fn matches(query: &str, items: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if item.contains(query) {
            out.push(item.clone());
        }
    }
    out
}
",
    ),
    (
        "scripts/score.py",
        "def score(values):
    total = sum(v * v for v in values)
    return total / len(values)
",
    ),
    (
        "web/app.ts",
        "export function count(items: string[]): number {
  return items.length;
}
",
    ),
    (
        "web/App.vue",
        "<template>
  <h1>{{ title }}</h1>
</template>
<script setup lang=\"ts\">
defineProps<{ title: string }>()
</script>
",
    ),
];

/// Temp git repo with Rust, Python, TypeScript and Vue files, one commit,
/// and one uncommitted change (to `web/app.ts`).
pub fn fixture_repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, body) in FILES {
        write(dir.path(), path, body);
    }
    let repo = git2::Repository::init(dir.path()).unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::now("Test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
        .unwrap();

    let ts = dir.path().join("web/app.ts");
    let changed = fs::read_to_string(&ts).unwrap() + "\nexport const VERSION = 2;\n";
    fs::write(ts, changed).unwrap();
    dir
}

fn write(root: &Path, path: &str, body: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}
