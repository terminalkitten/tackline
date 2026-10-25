//! Highlighted lines as HTML: the same highlighter and colors as the
//! terminal app, written as `<span style>`s.

use tuirealm::ratatui::style::{Color, Modifier};
use tuirealm::ratatui::text::Line;

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// CSS for a color: `#rrggbb`, or `var(--ansi-N)` for the palette themes
/// (`ansi`, `base16`), which use the page's terminal-like palette.
pub fn css_color(color: Color) -> Option<String> {
    match color {
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Indexed(n) => Some(format!("var(--ansi-{n})")),
        _ => None,
    }
}

pub fn line(line: &Line) -> String {
    let mut out = String::new();
    for span in &line.spans {
        let mut style = String::new();
        if let Some(fg) = span.style.fg.and_then(css_color) {
            style.push_str(&format!("color:{fg};"));
        }
        let m = span.style.add_modifier;
        if m.contains(Modifier::BOLD) {
            style.push_str("font-weight:bold;");
        }
        if m.contains(Modifier::ITALIC) {
            style.push_str("font-style:italic;");
        }
        if m.contains(Modifier::UNDERLINED) {
            style.push_str("text-decoration:underline;");
        }
        let text = escape(&span.content);
        if style.is_empty() {
            out.push_str(&text);
        } else {
            out.push_str(&format!("<span style=\"{style}\">{text}</span>"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::ratatui::style::Style;
    use tuirealm::ratatui::text::Span;

    #[test]
    fn spans_to_html() {
        let l = Line::from(vec![
            Span::styled(
                "fn",
                Style::new()
                    .fg(Color::Rgb(255, 0, 0))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" <a>"),
            Span::styled("x", Style::new().fg(Color::Indexed(3))),
        ]);
        assert_eq!(
            line(&l),
            "<span style=\"color:#ff0000;font-weight:bold;\">fn</span> &lt;a&gt;<span style=\"color:var(--ansi-3);\">x</span>"
        );
    }
}
