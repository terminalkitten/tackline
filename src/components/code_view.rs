//! Code pane, shared by the Files and Diff views. The model builds the rows;
//! this only draws them.

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyModifiers, NoUserEvent};
use tuirealm::props::{AttrValue, Attribute, PropPayload, Props, QueryResult};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Paragraph};
use tuirealm::state::State;
use two_face::re_exports::syntect::highlighting::Theme;

use super::{global_key, key, mouse};
use crate::app::{Motion, Msg, Popup, SelectionKind};
use crate::git::{Row, RowKind};
use crate::highlight::color;

/// What the pane shows, as set by the model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CodeProps {
    pub title: String,
    pub rows: Vec<Line<'static>>,
    pub focused: bool,
    /// Rows scrolled off the top.
    pub scroll: u16,
    /// Shown in the bottom border: the note under the cursor.
    pub footer: Option<String>,
}

impl CodeProps {
    pub fn message(title: &str, text: &str) -> Self {
        Self {
            title: title.to_string(),
            rows: vec![Line::styled(
                text.to_string(),
                Style::new().fg(Color::DarkGray),
            )],
            focused: false,
            scroll: 0,
            footer: None,
        }
    }
}

/// Line numbers, a space, then the note marker column and a space.
pub fn gutter_width(total_lines: usize) -> usize {
    total_lines.max(1).to_string().len() + 3
}

pub struct Input<'a> {
    pub title: String,
    /// Highlighted lines from `first_line` on.
    pub highlighted: &'a [Line<'static>],
    /// 0-based.
    pub first_line: usize,
    pub total_lines: usize,
    /// What each screen row shows: (line, first display column).
    pub rows: &'a [(usize, usize)],
    /// Rows starting past column 0 continue a wrapped line.
    pub wrap: bool,
    /// Visible code columns.
    pub width: usize,
    /// Cursor as (line, display column), when it should be drawn.
    pub cursor: Option<(usize, usize)>,
    pub current_line: usize,
    /// Diff rows from `first_line` on, when showing a diff.
    pub diff: Option<&'a [Row]>,
    /// Width of each line number column in a diff.
    pub diff_digits: usize,
    /// Lines covered by notes in the active session.
    pub marks: &'a [Mark],
    pub selection: Option<Selection>,
    /// Excerpt of the note under the cursor, for the bottom border.
    pub footer: Option<String>,
    pub theme: &'a Theme,
    pub focused: bool,
}

/// Note lines `start..=end` (0-based) get `●`, or `⚠` when the note is stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    pub start: usize,
    pub end: usize,
    pub stale: bool,
    /// Index of the note in the active session.
    pub note: usize,
}

/// From `start` to `end` inclusive, as (line, display column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: (usize, usize),
    pub end: (usize, usize),
    pub linewise: bool,
}

impl Selection {
    /// Selected display columns on line `n`, if any.
    fn cols(&self, n: usize) -> Option<(usize, usize)> {
        if n < self.start.0 || n > self.end.0 {
            return None;
        }
        if self.linewise {
            return Some((0, usize::MAX));
        }
        let from = if n == self.start.0 { self.start.1 } else { 0 };
        let to = if n == self.end.0 {
            self.end.1
        } else {
            usize::MAX
        };
        Some((from, to))
    }
}

pub fn build(input: Input) -> CodeProps {
    let settings = &input.theme.settings;
    let gutter = Style::new().fg(settings.gutter_foreground.map_or(Color::DarkGray, color));
    let current = Style::new().bg(settings.line_highlight.map_or(Color::Indexed(236), color));
    let selected = settings.selection.map_or(Color::Indexed(24), color);
    let (added, removed) = diff_colors(input.theme);
    let tacked = note_tint(input.theme);
    let digits = match input.diff {
        Some(_) => input.diff_digits,
        None => gutter_width(input.total_lines) - 3,
    };

    let rows = input
        .rows
        .iter()
        .map(|&(n, from)| {
            let i = n - input.first_line;
            let line = &input.highlighted[i];
            let row = input.diff.map(|d| &d[i]);
            let cursor = input.cursor.filter(|(l, _)| *l == n).map(|(_, x)| x);
            let marker = marker(input.marks, n);
            let sel = input.selection.and_then(|s| s.cols(n));
            let continued = input.wrap && from > 0;
            let mut spans = match row {
                _ if continued => {
                    let width = match input.diff {
                        Some(_) => diff_gutter_width(digits) - 2,
                        None => digits + 1,
                    };
                    vec![Span::raw(" ".repeat(width)), marker]
                }
                None => vec![Span::styled(format!("{:>digits$} ", n + 1), gutter), marker],
                Some(row) => {
                    let num = |n: Option<u32>| n.map_or(String::new(), |n| n.to_string());
                    let sign = match row.kind {
                        RowKind::Added => "+",
                        RowKind::Removed => "-",
                        RowKind::Context | RowKind::Hunk => " ",
                    };
                    let numbers =
                        format!("{:>digits$} {:>digits$} {sign}", num(row.old), num(row.new));
                    vec![Span::styled(numbers, gutter), marker]
                }
            };
            spans.push(Span::raw(" "));
            // Hunk headers aren't code: show them dim, unhighlighted.
            let hunk;
            let line = match row {
                Some(r) if r.kind == RowKind::Hunk => {
                    hunk = Line::styled(r.text.clone(), Style::new().fg(Color::DarkGray));
                    &hunk
                }
                _ => line,
            };
            let code = visible(line, from, input.width, cursor, sel, selected);
            let used: usize = code.iter().map(|s| s.content.chars().count()).sum();
            spans.extend(code);
            // Pad to the pane's width so row backgrounds (note tint, diff,
            // current line) fill the whole row, not just the text.
            spans.push(Span::raw(" ".repeat(input.width.saturating_sub(used))));
            let out = Line::from(spans);
            let diff_bg = match row.map(|r| r.kind) {
                Some(RowKind::Added) => Some(added),
                Some(RowKind::Removed) => Some(removed),
                _ => None,
            };
            let noted = input
                .marks
                .iter()
                .any(|m| !m.stale && (m.start..=m.end).contains(&n));
            if input.selection.is_some_and(|s| s.linewise) && sel.is_some() {
                out.style(Style::new().bg(selected))
            } else if noted {
                out.style(Style::new().bg(tacked))
            } else if let Some(bg) = diff_bg {
                out.style(Style::new().bg(bg))
            } else if n == input.current_line {
                out.style(current)
            } else {
                out
            }
        })
        .collect();
    CodeProps {
        title: input.title,
        rows,
        focused: input.focused,
        scroll: 0,
        footer: input.footer,
    }
}

/// Gutter glyph for line `n`: `●` on a note's first and last line, `│` in
/// between; yellow with `⚠` on the first line when stale. Where notes
/// overlap, an end beats a middle.
fn marker(marks: &[Mark], n: usize) -> Span<'static> {
    let covering: Vec<&Mark> = marks
        .iter()
        .filter(|m| (m.start..=m.end).contains(&n))
        .collect();
    if covering.is_empty() {
        return Span::raw(" ");
    }
    let stale = covering.iter().any(|m| m.stale);
    let color = if stale {
        Color::LightRed
    } else {
        Color::Yellow
    };
    let glyph = if covering.iter().any(|m| m.stale && m.start == n) {
        "⚠"
    } else if covering.iter().any(|m| m.start == n || m.end == n) {
        "●"
    } else {
        "│"
    };
    Span::styled(glyph, Style::new().fg(color))
}

/// Gutter width for a diff: old and new line numbers, sign, marker, space.
pub fn diff_gutter_width(digits: usize) -> usize {
    2 * digits + 5
}

/// Digits needed for the largest line number in `rows`.
pub fn diff_digits(rows: &[Row]) -> usize {
    let max = rows
        .iter()
        .flat_map(|r| [r.old, r.new])
        .flatten()
        .max()
        .unwrap_or(1);
    max.to_string().len()
}

/// Subtle added/removed backgrounds that suit the theme's brightness.
/// Light yellow behind noted ("tacked") lines, dim enough to read code on.
fn note_tint(theme: &Theme) -> Color {
    if is_light(theme) {
        Color::Rgb(255, 246, 204)
    } else {
        Color::Rgb(58, 52, 28)
    }
}

fn is_light(theme: &Theme) -> bool {
    theme
        .settings
        .background
        .is_some_and(|c| u32::from(c.r) + u32::from(c.g) + u32::from(c.b) > 3 * 128)
}

fn diff_colors(theme: &Theme) -> (Color, Color) {
    if is_light(theme) {
        (Color::Rgb(218, 251, 225), Color::Rgb(255, 235, 233))
    } else {
        (Color::Rgb(26, 58, 36), Color::Rgb(74, 30, 34))
    }
}

/// Columns `left..left + width` of `line`, with the cursor cell reversed and
/// the `sel` columns (inclusive) on the `selected` background.
fn visible(
    line: &Line<'static>,
    left: usize,
    width: usize,
    cursor: Option<usize>,
    sel: Option<(usize, usize)>,
    selected: Color,
) -> Vec<Span<'static>> {
    let mut cells: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();
    if let Some((from, to)) = sel {
        for (x, cell) in cells.iter_mut().enumerate() {
            if (from..=to).contains(&x) {
                cell.1 = cell.1.bg(selected);
            }
        }
    }
    if let Some(x) = cursor {
        if x >= cells.len() {
            cells.resize(x + 1, (' ', Style::new()));
        }
        cells[x].1 = cells[x].1.add_modifier(Modifier::REVERSED);
    }
    // Merge runs of equal style back into spans.
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (c, style) in cells.into_iter().skip(left).take(width) {
        match spans.last_mut() {
            Some(last) if last.style == style => last.content.to_mut().push(c),
            _ => spans.push(Span::styled(c.to_string(), style)),
        }
    }
    spans
}

/// Draw `CodeProps` from `props`' `Value` in a bordered block.
pub fn render(props: &Props, frame: &mut Frame, area: Rect) {
    let Some(code) = props
        .get(Attribute::Value)
        .and_then(AttrValue::as_payload)
        .and_then(PropPayload::as_any)
        .and_then(|p| p.downcast_ref::<CodeProps>())
    else {
        return;
    };
    let border = if code.focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(format!(" {} ", code.title));
    if let Some(footer) = &code.footer {
        block = block.title_bottom(Line::styled(
            format!(" ● {footer} "),
            Style::new().fg(Color::Yellow),
        ));
    }
    let paragraph = Paragraph::new(code.rows.clone())
        .block(block)
        .scroll((code.scroll, 0));
    frame.render_widget(paragraph, area);
}

/// A bordered pane that draws the `CodeProps` in its `Value`.
#[derive(Default)]
pub struct Rows {
    props: Props,
}

impl Component for Rows {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        render(&self.props, frame, area);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.props.get_for_query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        CmdResult::Invalid(cmd)
    }
}

#[derive(Component, Default)]
pub struct CodeView {
    component: Rows,
}

impl AppComponent<Msg, NoUserEvent> for CodeView {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        if let Some(msg) = mouse(ev) {
            return Some(msg);
        }
        // Shift+arrows select: up/down by line, left/right by char.
        let k = ev.as_keyboard()?;
        if k.modifiers.contains(KeyModifiers::SHIFT) {
            let extend = |kind, motion| Some(Msg::ExtendSelection(kind, motion));
            match k.code {
                Key::Up => return extend(SelectionKind::Line, Motion::Up),
                Key::Down => return extend(SelectionKind::Line, Motion::Down),
                Key::Left => return extend(SelectionKind::Char, Motion::Left),
                Key::Right => return extend(SelectionKind::Char, Motion::Right),
                _ => {}
            }
        }
        let motion = match key(ev)? {
            (Key::Down | Key::Char('j'), false) => Motion::Down,
            (Key::Up | Key::Char('k'), false) => Motion::Up,
            (Key::Left | Key::Char('h'), false) => Motion::Left,
            (Key::Right | Key::Char('l'), false) => Motion::Right,
            (Key::Char('G'), false) | (Key::End, true) => Motion::Bottom,
            (Key::Home, true) => Motion::Top,
            (Key::Char('d'), true) | (Key::PageDown, _) => Motion::HalfPageDown,
            (Key::Char('u'), true) | (Key::PageUp, _) => Motion::HalfPageUp,
            (Key::Char('0') | Key::Home, false) => Motion::LineStart,
            (Key::Char('$') | Key::End, false) => Motion::LineEnd,
            (Key::Char('g'), false) => return Some(Msg::PressG),
            (Key::Char('v'), false) => return Some(Msg::ToggleSelection(SelectionKind::Char)),
            (Key::Char('V'), false) => return Some(Msg::ToggleSelection(SelectionKind::Line)),
            (Key::Esc, _) => return Some(Msg::ClearSelection),
            (Key::Char('c'), false) => return Some(Msg::AddNote),
            (Key::Char('d'), false) => return Some(Msg::AskDeleteNote),
            (Key::Enter, false) => return Some(Msg::ViewNote),
            (Key::Char('r'), false) => return Some(Msg::RefreshDiff),
            (Key::Char('w'), false) => return Some(Msg::ToggleWrap),
            (Key::Char(':'), false) => return Some(Msg::OpenPopup(Popup::GotoLine)),
            (Key::Char('/'), false) => return Some(Msg::OpenPopup(Popup::FindInFile)),
            (Key::Char('n'), false) => return Some(Msg::FindNext),
            (Key::Char('N'), false) => return Some(Msg::FindPrev),
            (Key::Tab, _) => return Some(Msg::ToggleFocus),
            _ => return global_key(ev),
        };
        Some(Msg::Move(motion))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_cap_spans_and_dot_single_lines() {
        let mark = |start, end, stale| Mark {
            start,
            end,
            stale,
            note: 0,
        };
        let marks = [mark(0, 3, false), mark(5, 5, false), mark(7, 9, true)];
        let glyphs: String = (0..11)
            .map(|n| marker(&marks, n).content.to_string())
            .collect();
        assert_eq!(glyphs, "●││● ● ⚠│● ");
        // Overlap: the inner note's ends show through the outer one's line.
        let marks = [mark(0, 4, false), mark(2, 2, false)];
        let glyphs: String = (0..5)
            .map(|n| marker(&marks, n).content.to_string())
            .collect();
        assert_eq!(glyphs, "●│●│●");
    }

    #[test]
    fn visible_slices_and_marks_cursor() {
        let line = Line::from(vec![
            Span::raw("let "),
            Span::styled("x", Style::new().fg(Color::Red)),
        ]);
        let text = |spans: Vec<Span>| {
            spans
                .iter()
                .map(|s| s.content.to_string())
                .collect::<String>()
        };
        assert_eq!(text(visible(&line, 2, 10, None, None, Color::Blue)), "t x");
        let spans = visible(&line, 0, 10, Some(4), None, Color::Blue);
        assert_eq!(
            spans.last().unwrap().style,
            Style::new().fg(Color::Red).add_modifier(Modifier::REVERSED)
        );
        // Cursor past the end (empty line) still shows a cell.
        assert_eq!(
            text(visible(&Line::default(), 0, 10, Some(0), None, Color::Blue)),
            " "
        );
    }
}
