//! Diff view's file list. The diff itself shows in the code view.

use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};

use super::code_view::Rows;
use super::{global_key, key, wheel};
use crate::app::Msg;

#[derive(Component, Default)]
pub struct DiffFiles {
    component: Rows,
}

impl AppComponent<Msg, NoUserEvent> for DiffFiles {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        if let Some(msg) = wheel(ev) {
            return Some(msg);
        }
        match key(ev)? {
            (Key::Down | Key::Char('j'), false) => Some(Msg::DiffMove(1)),
            (Key::Up | Key::Char('k'), false) => Some(Msg::DiffMove(-1)),
            (Key::Enter | Key::Right | Key::Char('l') | Key::Tab | Key::BackTab, _) => {
                Some(Msg::ToggleFocus)
            }
            (Key::Char('r'), false) => Some(Msg::RefreshDiff),
            _ => global_key(ev),
        }
    }
}
