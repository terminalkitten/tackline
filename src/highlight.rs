//! Syntax detection (syntect + two-face). Highlighting itself lands with the
//! code viewer.

use std::path::Path;
use std::sync::OnceLock;

use two_face::re_exports::syntect::parsing::{SyntaxReference, SyntaxSet};

pub fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

/// Detect by extension (or full file name, e.g. `Dockerfile`), then by
/// first line (shebang).
pub fn detect(path: &Path, first_line: &str) -> Option<&'static SyntaxReference> {
    let set = syntax_set();
    let by_name = |s: Option<&std::ffi::OsStr>| {
        s.and_then(|s| s.to_str())
            .and_then(|s| set.find_syntax_by_extension(s))
    };
    by_name(path.extension())
        .or_else(|| by_name(path.file_name()))
        .or_else(|| set.find_syntax_by_first_line(first_line))
}

/// Code fence language: the syntax's primary extension (`rs`, `py`, `ts`).
pub fn fence_lang(path: &Path, first_line: &str) -> &'static str {
    detect(path, first_line)
        .and_then(|s| s.file_extensions.first())
        .map_or("", String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_langs() {
        let lang = |p: &str, first: &str| fence_lang(Path::new(p), first);
        assert_eq!(lang("src/main.rs", ""), "rs");
        assert_eq!(lang("app.py", ""), "py");
        assert_eq!(lang("web/app.ts", ""), "ts");
        assert_eq!(lang("web/App.vue", ""), "vue");
        assert_eq!(lang("bin/run", "#!/usr/bin/env python3"), "py");
        assert_eq!(lang("notes.unknownext", "hello"), "");
    }
}
