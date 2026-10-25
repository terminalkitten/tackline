//! Syntax detection and highlighting (syntect + two-face).

use std::path::Path;
use std::sync::OnceLock;

use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use two_face::re_exports::syntect::highlighting::{
    FontStyle, HighlightIterator, HighlightState, Highlighter, Theme,
};
use two_face::re_exports::syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

/// Tabs render as this many spaces.
pub const TAB: &str = "    ";

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

/// `name` if given (`None` when unknown), else dark `base16-ocean.dark` or
/// light `InspiredGitHub` depending on the `COLORFGBG` hint.
pub fn load_theme(name: Option<&str>) -> Option<Theme> {
    let name = match name {
        Some(n) => *EmbeddedLazyThemeSet::theme_names()
            .iter()
            .find(|t| t.as_name().eq_ignore_ascii_case(n))?,
        None if light_terminal(std::env::var("COLORFGBG").ok().as_deref()) => {
            EmbeddedThemeName::InspiredGithub
        }
        None => EmbeddedThemeName::Base16OceanDark,
    };
    Some(two_face::theme::extra().get(name).clone())
}

pub fn theme_names() -> impl Iterator<Item = &'static str> {
    EmbeddedLazyThemeSet::theme_names()
        .iter()
        .map(|t| t.as_name())
}

/// `COLORFGBG` is `fg;bg` (sometimes `fg;default;bg`); bg 7 or 9-15 is light.
fn light_terminal(colorfgbg: Option<&str>) -> bool {
    colorfgbg
        .and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok())
        .is_some_and(|bg| matches!(bg, 7 | 9..=15))
}

/// Lazily highlighted lines of one file: only up to the furthest line asked
/// for, continuing from the saved parser state.
// ponytail: every highlighted line stays cached, so the spec's 200-line
// parse-state checkpoints would never be read; add them if lines get evicted.
pub struct Highlighted {
    parse: ParseState,
    state: HighlightState,
    pub lines: Vec<Line<'static>>,
}

impl Highlighted {
    pub fn new(path: &Path, first_line: &str, theme: &Theme) -> Self {
        let syntax =
            detect(path, first_line).unwrap_or_else(|| syntax_set().find_syntax_plain_text());
        Self {
            parse: ParseState::new(syntax),
            state: HighlightState::new(&Highlighter::new(theme), ScopeStack::new()),
            lines: Vec::new(),
        }
    }

    /// Highlight `text` up to line `n` (exclusive, 0-based).
    pub fn ensure(&mut self, text: &[String], n: usize, theme: &Theme) {
        let highlighter = Highlighter::new(theme);
        while self.lines.len() < n.min(text.len()) {
            // The `extra_newlines` syntaxes expect each line to end in `\n`.
            let line = format!("{}\n", text[self.lines.len()]);
            let ops = self
                .parse
                .parse_line(&line, syntax_set())
                .unwrap_or_default();
            let spans = HighlightIterator::new(&mut self.state, &ops, &line, &highlighter)
                .map(|(style, piece)| {
                    Span::styled(
                        piece.trim_end_matches('\n').replace('\t', TAB),
                        to_style(style),
                    )
                })
                .filter(|span| !span.content.is_empty())
                .collect::<Vec<_>>();
            self.lines.push(Line::from(spans));
        }
    }
}

/// syntect → ratatui. The background is left to the terminal, like bat.
// ponytail: hand-rolled because syntect-tui 3.0 targets ratatui 0.29, not
// tuirealm's 0.30; swap it in when it catches up.
fn to_style(s: two_face::re_exports::syntect::highlighting::Style) -> Style {
    let mut style = Style::default().fg(color(s.foreground));
    for (font, modifier) in [
        (FontStyle::BOLD, Modifier::BOLD),
        (FontStyle::ITALIC, Modifier::ITALIC),
        (FontStyle::UNDERLINE, Modifier::UNDERLINED),
    ] {
        if s.font_style.contains(font) {
            style = style.add_modifier(modifier);
        }
    }
    style
}

/// Whether the theme has a light background.
pub fn is_light(theme: &Theme) -> bool {
    theme
        .settings
        .background
        .is_some_and(|c| c.a >= 2 && u32::from(c.r) + u32::from(c.g) + u32::from(c.b) > 3 * 128)
}

/// A theme color as a ratatui color. The `ansi`, `base16` and `base16-256`
/// themes use the terminal's palette: alpha 0 means color number `r`, alpha 1
/// the terminal's default (the same encoding bat uses).
pub fn color(c: two_face::re_exports::syntect::highlighting::Color) -> Color {
    match c.a {
        0 => Color::Indexed(c.r),
        1 => Color::Reset,
        _ => Color::Rgb(c.r, c.g, c.b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn highlights_fixture_languages_in_color() {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let repo = crate::testutil::fixture_repo();
        for file in [
            "src/matcher.rs",
            "scripts/score.py",
            "web/app.ts",
            "web/App.vue",
        ] {
            let text = lines(&std::fs::read_to_string(repo.path().join(file)).unwrap());
            let mut h = Highlighted::new(Path::new(file), &text[0], &theme);
            h.ensure(&text, usize::MAX, &theme);
            let colors: std::collections::HashSet<_> = h
                .lines
                .iter()
                .flat_map(|l| l.spans.iter().map(|s| s.style.fg))
                .collect();
            assert!(colors.len() >= 3, "{file}: only {colors:?}");
            let plain: Vec<String> = h.lines.iter().map(|l| l.to_string()).collect();
            assert_eq!(plain, text, "{file}");
        }
    }

    #[test]
    fn highlighting_is_lazy_and_resumable() {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let text = lines("fn a() {}\n\tlet s = \"x\";\nfn b() {}\nfn c() {}");
        let mut h = Highlighted::new(Path::new("x.rs"), &text[0], &theme);
        h.ensure(&text, 2, &theme);
        assert_eq!(h.lines.len(), 2);
        assert_eq!(h.lines[1].to_string(), "    let s = \"x\";");
        h.ensure(&text, 100, &theme);
        assert_eq!(h.lines.len(), 4);
    }

    #[test]
    fn palette_themes_use_terminal_colors() {
        let theme = load_theme(Some("ansi")).unwrap();
        let text = lines("fn main() { let x = 1; }");
        let mut h = Highlighted::new(Path::new("x.rs"), &text[0], &theme);
        h.ensure(&text, 1, &theme);
        let colors: Vec<_> = h.lines[0].spans.iter().filter_map(|s| s.style.fg).collect();
        assert!(
            colors
                .iter()
                .all(|c| matches!(c, Color::Indexed(0..=15) | Color::Reset)),
            "{colors:?}"
        );
        assert!(colors.iter().any(|c| matches!(c, Color::Indexed(_))));
    }

    #[test]
    fn themes() {
        assert!(load_theme(Some("InspiredGitHub")).is_some());
        assert!(load_theme(Some("nope")).is_none());
        assert!(light_terminal(Some("0;15")));
        assert!(light_terminal(Some("0;default;7")));
        assert!(!light_terminal(Some("15;0")));
        assert!(!light_terminal(None));
    }

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
