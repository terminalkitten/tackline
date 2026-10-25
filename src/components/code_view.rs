//! Code pane, shared by the Files and Diff views. The model builds the rows;
//! this only draws them.

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{AttrValue, Attribute, PropPayload, Props, QueryResult};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::{Line, Span};
use tuirealm::ratatui::widgets::{Block, BorderType, Paragraph};
use tuirealm::state::State;
use two_face::re_exports::syntect::highlighting::Theme;

use super::{global_key, key};
use crate::app::{Motion, Msg};
use crate::highlight::color;

/// What the pane shows, as set by the model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CodeProps {
    pub title: String,
    pub rows: Vec<Line<'static>>,
    pub focused: bool,
    /// Rows scrolled off the top.
    pub scroll: u16,
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
        }
    }
}

/// Line numbers, a space, then the note marker column and a space.
pub fn gutter_width(total_lines: usize) -> usize {
    total_lines.max(1).to_string().len() + 3
}

pub struct Input<'a> {
    pub title: String,
    /// Highlighted lines from `first_line` on, one per visible row.
    pub highlighted: &'a [Line<'static>],
    /// 0-based.
    pub first_line: usize,
    pub total_lines: usize,
    /// First visible display column.
    pub left: usize,
    /// Visible code columns.
    pub width: usize,
    /// Cursor as (line, display column), when it should be drawn.
    pub cursor: Option<(usize, usize)>,
    pub current_line: usize,
    pub theme: &'a Theme,
    pub focused: bool,
}

pub fn build(input: Input) -> CodeProps {
    let digits = gutter_width(input.total_lines) - 3;
    let settings = &input.theme.settings;
    let gutter = Style::new().fg(settings.gutter_foreground.map_or(Color::DarkGray, color));
    let current = Style::new().bg(settings.line_highlight.map_or(Color::Indexed(236), color));

    let rows = input
        .highlighted
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let n = input.first_line + i;
            let cursor = input.cursor.filter(|(l, _)| *l == n).map(|(_, x)| x);
            let mut spans = vec![Span::styled(format!("{:>digits$}   ", n + 1), gutter)];
            spans.extend(visible(line, input.left, input.width, cursor));
            let row = Line::from(spans);
            if n == input.current_line {
                row.style(current)
            } else {
                row
            }
        })
        .collect();
    CodeProps {
        title: input.title,
        rows,
        focused: input.focused,
        scroll: 0,
    }
}

/// Columns `left..left + width` of `line`, with the cursor cell reversed.
fn visible(
    line: &Line<'static>,
    left: usize,
    width: usize,
    cursor: Option<usize>,
) -> Vec<Span<'static>> {
    let mut cells: Vec<(char, Style)> = line
        .spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();
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
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(format!(" {} ", code.title));
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
            (Key::Tab | Key::BackTab, _) => return Some(Msg::ToggleFocus),
            _ => return global_key(ev),
        };
        Some(Msg::Move(motion))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(text(visible(&line, 2, 10, None)), "t x");
        let spans = visible(&line, 0, 10, Some(4));
        assert_eq!(
            spans.last().unwrap().style,
            Style::new().fg(Color::Red).add_modifier(Modifier::REVERSED)
        );
        // Cursor past the end (empty line) still shows a cell.
        assert_eq!(text(visible(&Line::default(), 0, 10, Some(0))), " ");
    }
}
