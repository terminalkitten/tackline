//! File tree (tui-realm-treeview). Which directories are open and which row
//! is selected is widget state; opening a file goes through `Msg::OpenFile`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tui_realm_treeview::{
    Node, NodeValue, TREE_CMD_CLOSE, TREE_CMD_OPEN, TREE_INITIAL_NODE, Tree, TreeState, TreeView,
};
use tuirealm::command::{Cmd, CmdResult, Direction};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, MouseButton, MouseEvent, MouseEventKind, NoUserEvent};
use tuirealm::props::{
    AttrValue, Attribute, HorizontalAlignment, PropPayload, PropValue, QueryResult, Style, Title,
};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::layout::Rect;
use tuirealm::ratatui::style::{Color, Modifier};
use tuirealm::state::{State, StateValue};

use super::{borders, global_key, key, mouse};
use crate::app::Msg;

/// Node id of the root.
pub const ROOT: &str = "/";

/// Build the tree from `walk` output. Ids are relative paths, with a trailing
/// `/` for directories; directories sort before files.
/// A tree row: the name, and a yellow `●` when the file has notes. The
/// name has no style of its own, so it keeps the row's (selection) style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Label {
    pub name: String,
    pub marked: bool,
}

impl Label {
    fn new(name: String) -> Self {
        Self {
            name,
            marked: false,
        }
    }
}

impl NodeValue for Label {
    fn render_parts_iter(&self) -> impl Iterator<Item = (&str, Option<Style>)> {
        let marker = (" ●", Some(Style::new().fg(Color::Yellow)));
        std::iter::once((self.name.as_str(), None)).chain(self.marked.then_some(marker))
    }
}

pub fn build_tree(root_name: &str, entries: &[(PathBuf, bool)]) -> Tree<Label> {
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
    ) -> Node<Label> {
        let name = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if !is_dir {
            return Node::new(path.display().to_string(), Label::new(name));
        }
        let mut kids = children.get(path).cloned().unwrap_or_default();
        kids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let kids = kids
            .into_iter()
            .map(|(p, d)| node(p, d, children))
            .collect();
        Node::new(
            format!("{}/", path.display()),
            Label::new(format!("{name}/")),
        )
        .with_children(kids)
    }
    // The root's id comes out as `ROOT`; show the repo's folder name.
    let mut root = node(Path::new(""), true, &children);
    root.set_value(Label::new(format!("{root_name}/")));
    Tree::new(root)
}

/// Attribute with the ids of files to mark (`PropPayload::Vec` of `Str`).
pub const MARKED: &str = "marked";

/// Attribute: whether the tree is on screen (only then does it take clicks).
pub const VISIBLE: &str = "visible";

pub struct FileTree {
    component: TreeView<Label>,
    marked: Vec<String>,
    /// Where the tree was last drawn, for mapping clicks to rows.
    area: Option<Rect>,
    visible: bool,
    /// Has focus (tuirealm sets `Attribute::Focus`).
    focused: bool,
}

/// Everything goes to the tree view, except `MARKED`, which relabels files.
impl Component for FileTree {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.area = Some(area);
        self.component.view(frame, area);
    }

    fn query<'a>(&'a self, attr: Attribute) -> Option<QueryResult<'a>> {
        self.component.query(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        if attr == Attribute::Custom(VISIBLE) {
            self.visible = matches!(value, AttrValue::Flag(true));
            return;
        }
        if attr == Attribute::Focus {
            self.focused = matches!(value, AttrValue::Flag(true));
        }
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
        for (ids, on) in [(&self.marked, false), (&marked, true)] {
            for id in ids {
                if let Some(node) = root.query_mut(id) {
                    let label = Label {
                        marked: on,
                        ..node.value().clone()
                    };
                    node.set_value(label);
                }
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
    pub fn new(tree: Tree<Label>) -> Self {
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
            area: None,
            visible: true,
            focused: false,
        }
    }

    /// Node ids in the order they're drawn: open folders show their children.
    fn rows(&self) -> Vec<String> {
        fn visit(node: &Node<Label>, state: &TreeState, out: &mut Vec<String>) {
            out.push(node.id().clone());
            if !state.is_closed(node) {
                node.iter().for_each(|child| visit(child, state, out));
            }
        }
        let mut out = Vec::new();
        visit(
            self.component.tree().root(),
            self.component.tree_state(),
            &mut out,
        );
        out
    }

    /// The node drawn at screen row `y`, using the tree view's own scroll
    /// rule (the selected row sits mid-way, within bounds).
    fn node_at(&self, y: u16) -> Option<String> {
        let inner = self
            .area?
            .inner(tuirealm::ratatui::layout::Margin::new(1, 1));
        if y < inner.y || y >= inner.bottom() {
            return None;
        }
        let rows = self.rows();
        let height = inner.height as usize;
        let selected = self.component.tree_state().selected();
        let selected = rows
            .iter()
            .position(|id| Some(id.as_str()) == selected)
            .map_or(0, |i| i + 1);
        let skip = selected
            .saturating_sub(height / 2)
            .min(rows.len().saturating_sub(height));
        rows.get(skip + (y - inner.y) as usize).cloned()
    }

    /// Mouse on the tree: click selects and opens, the wheel moves.
    fn mouse(&mut self, ev: &MouseEvent) -> Option<Msg> {
        let area = self.area?;
        if !self.visible
            || !area.contains(tuirealm::ratatui::layout::Position::new(ev.column, ev.row))
        {
            return None;
        }
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let id = self.node_at(ev.row)?;
                self.component
                    .attr(Attribute::Custom(TREE_INITIAL_NODE), AttrValue::String(id));
                self.enter()
            }
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let dir = if ev.kind == MouseEventKind::ScrollDown {
                    Direction::Down
                } else {
                    Direction::Up
                };
                (0..3).for_each(|_| {
                    self.component.perform(Cmd::Move(dir));
                });
                Some(Msg::Redraw)
            }
            _ => None,
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
        if let Event::Mouse(m) = ev {
            // On the tree: handled here. Elsewhere the model routes it, but
            // only from here when the tree has focus; otherwise the focused
            // pane already passed it on (the tree sees all mouse events
            // through its subscription).
            return self
                .mouse(m)
                .or_else(|| if self.focused { mouse(ev) } else { None });
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
    fn marker_is_yellow_and_the_name_keeps_the_row_style() {
        let label = Label {
            name: "matcher.rs".into(),
            marked: true,
        };
        let parts: Vec<_> = label.render_parts_iter().collect();
        assert_eq!(
            parts,
            [
                ("matcher.rs", None),
                (" ●", Some(Style::new().fg(Color::Yellow)))
            ]
        );
        let plain = Label::new("x.rs".into());
        assert_eq!(plain.render_parts_iter().count(), 1);
    }

    fn click(tree: &mut FileTree, col: u16, row: u16) -> Option<Msg> {
        tree.on(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            modifiers: tuirealm::event::KeyModifiers::NONE,
            column: col,
            row,
        }))
    }

    #[test]
    fn click_opens_folders_and_files() {
        let repo = crate::testutil::fixture_repo();
        let mut tree = FileTree::new(build_tree("repo", &crate::search::files::walk(repo.path())));
        // Drawn at 0,0 (30x8): border on row 0, then repo/, scripts/, src/, web/.
        render_to_string(&mut tree, Size::new(30, 8));
        assert_eq!(click(&mut tree, 6, 3), Some(Msg::Redraw), "src/ opens");
        render_to_string(&mut tree, Size::new(30, 8));
        assert_eq!(
            click(&mut tree, 8, 4),
            Some(Msg::OpenFile("src/matcher.rs".into()))
        );
        assert_eq!(
            click(&mut tree, 6, 3),
            Some(Msg::Redraw),
            "src/ closes again"
        );
        render_to_string(&mut tree, Size::new(30, 8));
        assert_eq!(click(&mut tree, 6, 6), None, "below the last row: nothing");

        // Outside the tree: passed on only when the tree has focus.
        assert_eq!(click(&mut tree, 50, 3), None);
        tree.attr(Attribute::Focus, AttrValue::Flag(true));
        assert_eq!(
            click(&mut tree, 50, 3),
            Some(Msg::Click { col: 50, row: 3 })
        );
        // Not on screen (another view or a popup): no clicks.
        tree.attr(Attribute::Custom(VISIBLE), AttrValue::Flag(false));
        assert_eq!(click(&mut tree, 6, 3), Some(Msg::Click { col: 6, row: 3 }));
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
