use tui_realm_stdlib::components::Span as SpanComponent;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, NoUserEvent};
use tuirealm::props::{AttrValue, PropPayload, PropValue};
use tuirealm::ratatui::style::{Color, Modifier, Style};
use tuirealm::ratatui::text::Span;

use crate::app::Msg;
use crate::model::View;

/// View, active session, file and cursor, key hints.
pub fn spans(
    view: View,
    session: Option<(&str, usize)>,
    location: Option<String>,
) -> Vec<Span<'static>> {
    let view = match view {
        View::Files => " FILES ",
        View::Diff => " DIFF ",
        View::Session => " SESSION ",
    };
    let session = match session {
        Some((name, 1)) => format!(" {name} (1 note) "),
        Some((name, n)) => format!(" {name} ({n} notes) "),
        None => " no session ".to_string(),
    };
    let dim = Style::new().fg(Color::DarkGray);
    vec![
        Span::styled(
            view,
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(session, Style::new().fg(Color::Cyan)),
        Span::styled(
            location.map(|l| format!(" {l} ")).unwrap_or_default(),
            Style::new(),
        ),
        Span::styled(" │ ? help  q quit", dim),
    ]
}

pub fn attr(spans: Vec<Span<'static>>) -> AttrValue {
    AttrValue::Payload(PropPayload::Vec(
        spans.into_iter().map(PropValue::TextSpan).collect(),
    ))
}

#[derive(Component, Default)]
pub struct StatusBar {
    component: SpanComponent,
}

impl AppComponent<Msg, NoUserEvent> for StatusBar {
    fn on(&mut self, _: &Event<NoUserEvent>) -> Option<Msg> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::props::Attribute;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    #[test]
    fn renders() {
        let mut bar = StatusBar::default();
        bar.attr(
            Attribute::Text,
            attr(spans(
                View::Files,
                Some(("perf", 3)),
                Some("src/a.rs 2:5".into()),
            )),
        );
        insta::assert_snapshot!(render_to_string(&mut bar, Size::new(70, 1)));
    }
}
