//! File tree (tui-realm-treeview). Which directories are open and which row
//! is selected is widget state; opening a file goes through `Msg::OpenFile`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tui_realm_treeview::{Node, TREE_CMD_CLOSE, TREE_CMD_OPEN, Tree, TreeView};
use tuirealm::command::{Cmd, CmdResult, Direction};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, NoUserEvent};
use tuirealm::props::{
    AttrValue, Attribute, HorizontalAlignment, PropPayload, PropValue, QueryResult, Style, Title,
};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::style::Modifier;
use tuirealm::state::{State, StateValue};

use super::{borders, global_key, key, mouse};
use crate::app::Msg;

/// Node id of the root.
pub const ROOT: &str = "/";

/// Build the tree from `walk` output. Ids are relative paths, with a trailing
/// `/` for directories; directories sort before files.
pub fn build_tree(root_name: &str, entries: &[(PathBuf, bool)]) -> Tree<String> {
    let mut children: BTreeMap<&Path, Vec<(&Path, bool)>> = BTreeMap::new();
    for (path, is_dir) in entries {
        children
            .entry(path.parent().unwrap_or(Path::new("")))
            .or_default()
            .push((path, *is_dir));
    }
    fn node(
        path: &Path,
        is_dir: bool,
        children: &BTreeMap<&Path, Vec<(&Path, bool)>>,
    ) -> Node<String> {
        let name = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if !is_dir {
            return Node::new(path.display().to_string(), name);
        }
        let mut kids = children.get(path).cloned().unwrap_or_default();
        kids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let kids = kids
            .into_iter()
            .map(|(p, d)| node(p, d, children))
            .collect();
        Node::new(format!("{}/", path.display()), format!("{name}/")).with_children(kids)
    }
    // The root's id comes out as `ROOT`; show the repo's folder name.
    let mut root = node(Path::new(""), true, &children);
    root.set_value(format!("{root_name}/"));
    Tree::new(root)
}

/// Attribute with the ids of files to mark (`PropPayload::Vec` of `Str`).
pub const MARKED: &str = "marked";
const MARKER: &str = " ●";

pub struct FileTree {
    component: TreeView<String>,
    marked: Vec<String>,
}

/// Everything goes to the tree view, except `MARKED`, which relabels files.
impl Component for FileTree {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.component.view(frame, area);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.component.query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        if attr != Attribute::Custom(MARKED) {
            return self.component.attr(attr, value);
        }
        let marked: Vec<String> = match value {
            AttrValue::Payload(PropPayload::Vec(ids)) => ids
                .into_iter()
                .filter_map(|v| match v {
                    PropValue::Str(id) => Some(id),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        if marked == self.marked {
            return;
        }
        let root = self.component.tree_mut().root_mut();
        for id in &self.marked {
            if let Some(node) = root.query_mut(id) {
                let name = node.value().trim_end_matches(MARKER).to_string();
                node.set_value(name);
            }
        }
        for id in &marked {
            if let Some(node) = root.query_mut(id) {
                let name = format!("{}{MARKER}", node.value());
                node.set_value(name);
            }
        }
        self.marked = marked;
    }

    fn state(&self) -> State {
        self.component.state()
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.component.perform(cmd)
    }
}

impl FileTree {
    pub fn new(tree: Tree<String>) -> Self {
        let mut component = TreeView::default()
            .borders(borders(true))
            .inactive(Style::new())
            .title(Title::from(" Files ").alignment(HorizontalAlignment::Left))
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
            .highlight_style_inactive(Style::new().add_modifier(Modifier::BOLD))
            .indent_size(2)
            .scroll_step(10)
            .with_tree(tree)
            .initial_node(ROOT);
        component.perform(Cmd::Custom(TREE_CMD_OPEN));
        Self {
            component,
            marked: Vec::new(),
        }
    }

    /// `Enter`: toggle a directory, open a file.
    fn enter(&mut self) -> Option<Msg> {
        let State::Single(StateValue::String(id)) = self.component.state() else {
            return None;
        };
        if !id.ends_with('/') {
            return Some(Msg::OpenFile(PathBuf::from(id)));
        }
        let open = self
            .component
            .tree()
            .root()
            .query(&id)
            .is_some_and(|n| self.component.tree_state().is_open(n));
        self.component.perform(Cmd::Custom(if open {
            TREE_CMD_CLOSE
        } else {
            TREE_CMD_OPEN
        }));
        Some(Msg::Redraw)
    }
}

impl AppComponent<Msg, NoUserEvent> for FileTree {
    fn on(&mut self, ev: &Event<NoUserEvent>) -> Option<Msg> {
        if let Some(msg) = mouse(ev) {
            return Some(msg);
        }
        let cmd = match key(ev)? {
            (Key::Down | Key::Char('j'), false) => Cmd::Move(Direction::Down),
            (Key::Up | Key::Char('k'), false) => Cmd::Move(Direction::Up),
            (Key::Right | Key::Char('l'), false) => Cmd::Custom(TREE_CMD_OPEN),
            (Key::Left | Key::Char('h'), false) => Cmd::Custom(TREE_CMD_CLOSE),
            (Key::Char('d'), true) | (Key::PageDown, _) => Cmd::Scroll(Direction::Down),
            (Key::Char('u'), true) | (Key::PageUp, _) => Cmd::Scroll(Direction::Up),
            (Key::Enter, false) => return self.enter(),
            (Key::Tab, _) => return Some(Msg::ToggleFocus),
            _ => return global_key(ev),
        };
        self.component.perform(cmd);
        Some(Msg::Redraw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuirealm::event::{KeyEvent, KeyModifiers};
    use tuirealm::ratatui::layout::Size;
    use tuirealm::testing::render_to_string;

    fn press(tree: &mut FileTree, code: Key) -> Option<Msg> {
        tree.on(&Event::Keyboard(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    #[test]
    fn navigate_toggle_and_open() {
        let repo = crate::testutil::fixture_repo();
        let mut tree = FileTree::new(build_tree("repo", &crate::search::files::walk(repo.path())));
        insta::assert_snapshot!(
            "tree_initial",
            render_to_string(&mut tree, Size::new(30, 8))
        );

        // scripts/ → src/, open it, down to src/matcher.rs, Enter opens.
        press(&mut tree, Key::Char('j'));
        press(&mut tree, Key::Char('j'));
        assert_eq!(press(&mut tree, Key::Enter), Some(Msg::Redraw));
        press(&mut tree, Key::Char('j'));
        assert_eq!(
            press(&mut tree, Key::Enter),
            Some(Msg::OpenFile("src/matcher.rs".into()))
        );
        insta::assert_snapshot!(
            "tree_src_open",
            render_to_string(&mut tree, Size::new(30, 8))
        );
        assert_eq!(press(&mut tree, Key::Char('q')), Some(Msg::Quit));

        let mark = |ids: &[&str]| {
            AttrValue::Payload(PropPayload::Vec(
                ids.iter().map(|i| PropValue::Str(i.to_string())).collect(),
            ))
        };
        tree.attr(Attribute::Custom(MARKED), mark(&["src/matcher.rs"]));
        insta::assert_snapshot!("tree_marked", render_to_string(&mut tree, Size::new(30, 8)));
        tree.attr(Attribute::Custom(MARKED), mark(&[]));
        assert!(!render_to_string(&mut tree, Size::new(30, 8)).contains('●'));
    }
}
