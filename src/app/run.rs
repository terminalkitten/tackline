//! Terminal loop: tuirealm events → `Msg` → `Model::update` → props → draw.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use crossterm::clipboard::CopyToClipboard;
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use tuirealm::application::{Application, PollStrategy};
use tuirealm::event::{KeyModifiers, MouseEventKind, NoUserEvent};
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{
    AttrValue, Attribute, HorizontalAlignment, PropBound, PropPayload, PropValue, Title,
};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::text::Text;
use tuirealm::ratatui::widgets::Clear;
use tuirealm::subscription::{EventClause, MouseEventClause, Sub, SubClause};
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

use super::{Id, Model, Msg, Popup, layout};
use crate::components::code_view::CodeView;
use crate::components::confirm::{self, Confirm};
use crate::components::diff_view::DiffFiles;
use crate::components::file_tree::{FileTree, MARKED, VISIBLE, build_tree};
use crate::components::finder_popup::{self, PickerResults, QueryInput};
use crate::components::help_popup::{self, HelpPopup};
use crate::components::name_input::NameInput;
use crate::components::note_popup::{self, NoteHeader, NoteText, NoteViewer};
use crate::components::session_picker::{self, SessionPicker};
use crate::components::session_view::SessionView;
use crate::components::status_bar::{self, StatusBar};
use crate::components::toast::{self, Toast};
use crate::components::{borders, centered};
use crate::model::View;
use crate::search::files::walk;

type App = Application<Id, Msg, NoUserEvent>;

const TOAST_TTL: Duration = Duration::from_secs(3);

pub fn run(mut model: Model) -> Result<()> {
    // `new` also installs a panic hook that leaves raw mode, the alternate
    // screen and mouse capture before the panic message prints.
    let mut term = CrosstermTerminalAdapter::new()?;
    term.enable_raw_mode()?;
    term.enter_alternate_screen()?;
    term.enable_mouse_capture()?;
    // Ask for unambiguous keys (Shift+Enter ≠ Enter) where the terminal
    // supports the kitty protocol. The flags belong to the alternate screen,
    // so leaving it (also on panic) drops them. Must run before the input
    // listener starts, or it could eat the terminal's reply.
    let enhanced = crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        crossterm::execute!(
            std::io::stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;
    }
    let mut app = mount(
        &model.root,
        // Poll often and take everything waiting: key repeat and trackpad
        // bursts would otherwise queue up (3 events / 20ms caps at ~150/s).
        EventListenerCfg::default().crossterm_input_listener(Duration::from_millis(5), 256),
    )?;
    let mut clipboard = None;

    let mut dirty = true;
    let mut shown_popup = None;
    while !model.quit {
        let size = term.raw().size()?;
        if (size.width, size.height) != model.size {
            model.update(Msg::Resize(size.width, size.height));
            dirty = true;
        }
        if model
            .toast
            .as_ref()
            .is_some_and(|t| t.at.elapsed() > TOAST_TTL)
        {
            model.update(Msg::DismissToast);
            dirty = true;
        }
        // The one effect `update` asks for instead of doing itself.
        if let Some(text) = model.clipboard.take() {
            if let Err(e) = copy(&mut clipboard, text) {
                model.update(Msg::Error(format!("Couldn't copy: {e}")));
            }
            dirty = true;
        }
        if dirty {
            let opened = model.popup.filter(|_| model.popup != shown_popup);
            shown_popup = model.popup;
            sync(&mut model, &mut app, opened, enhanced)?;
            term.draw(|f| draw(&mut app, &model, f))?;
        }
        // Keep drawing while highlighting catches up with the visible rows.
        dirty = model.highlight_pending();
        let wait = if dirty { 1 } else { 50 };
        // Apply every queued event, then draw once.
        for msg in app.tick(PollStrategy::UpTo(256, Duration::from_millis(wait)))? {
            model.update(msg);
            dirty = true;
        }
    }
    if enhanced {
        crossterm::execute!(std::io::stdout(), PopKeyboardEnhancementFlags)?;
    }
    term.restore()?;
    Ok(())
}

/// Copy with the OS clipboard (kept alive, so Linux clipboard owners don't
/// vanish with it). Over ssh, or when that fails, ask the terminal to copy
/// with OSC 52, which also reaches the local clipboard through ssh and tmux
/// (tmux needs `set -g set-clipboard on`).
fn copy(clipboard: &mut Option<arboard::Clipboard>, text: String) -> Result<()> {
    let remote = ["SSH_CONNECTION", "SSH_TTY"]
        .iter()
        .any(|v| std::env::var_os(v).is_some());
    if !remote {
        let mut os_copy = || -> Result<()> {
            if clipboard.is_none() {
                *clipboard = Some(arboard::Clipboard::new()?);
            }
            clipboard
                .as_mut()
                .expect("just set")
                .set_text(text.clone())?;
            Ok(())
        };
        if os_copy().is_ok() {
            return Ok(());
        }
    }
    crossterm::execute!(std::io::stdout(), CopyToClipboard::to_clipboard_from(text))?;
    Ok(())
}

fn mount(root: &Path, listener: EventListenerCfg<NoUserEvent>) -> Result<App> {
    let mut app = Application::init(listener);
    let name = root
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let tree = build_tree(&name, &walk(root));
    // The tree takes clicks and the wheel even without focus.
    let anywhere = MouseEventClause {
        kind: MouseEventKind::Moved,
        modifiers: KeyModifiers::NONE,
        column: 0..u16::MAX,
        row: 0..u16::MAX,
    };
    let mouse = Sub::new(EventClause::Mouse(anywhere), SubClause::Always);
    app.mount(Id::FileTree, Box::new(FileTree::new(tree)), vec![mouse])?;
    app.mount(Id::CodeView, Box::new(CodeView::default()), vec![])?;
    app.mount(Id::SessionView, Box::new(SessionView::default()), vec![])?;
    app.mount(Id::DiffFiles, Box::new(DiffFiles::default()), vec![])?;
    app.mount(Id::StatusBar, Box::new(StatusBar::default()), vec![])?;
    app.mount(Id::Help, Box::new(HelpPopup::default()), vec![])?;
    app.mount(Id::NameInput, Box::new(NameInput::default()), vec![])?;
    app.mount(
        Id::SessionPicker,
        Box::new(SessionPicker::default()),
        vec![],
    )?;
    app.mount(Id::Toast, Box::new(Toast::default()), vec![])?;
    app.mount(Id::NoteHeader, Box::new(NoteHeader::default()), vec![])?;
    app.mount(Id::NoteText, Box::new(NoteText::new("", false)), vec![])?;
    app.mount(Id::QueryInput, Box::new(QueryInput::default()), vec![])?;
    app.mount(
        Id::PickerResults,
        Box::new(PickerResults::default()),
        vec![],
    )?;
    app.mount(Id::Confirm, Box::new(Confirm::default()), vec![])?;
    app.mount(Id::NoteViewer, Box::new(NoteViewer::default()), vec![])?;
    Ok(app)
}

fn any(value: impl PropBound) -> AttrValue {
    AttrValue::Payload(PropPayload::Any(value.to_any_prop()))
}

/// Push the model's state into the components' props. `opened` is a popup
/// that just opened and starts fresh.
fn sync(model: &mut Model, app: &mut App, opened: Option<Popup>, shift_enter: bool) -> Result<()> {
    match opened {
        Some(Popup::NewSession) => app.attr(
            &Id::NameInput,
            Attribute::Value,
            AttrValue::String(String::new()),
        )?,
        Some(Popup::SessionPicker) => {
            let active = model.state.active_session_id.as_deref();
            let index = model
                .sessions
                .iter()
                .position(|s| Some(s.id.as_str()) == active);
            app.attr(
                &Id::SessionPicker,
                Attribute::Text,
                session_picker::rows(&model.sessions, active),
            )?;
            app.attr(
                &Id::SessionPicker,
                Attribute::Value,
                session_picker::selected(index.unwrap_or(0)),
            )?;
        }
        Some(Popup::Note) => {
            let text = model.draft.as_ref().map_or("", |d| d.text.as_str());
            app.remount(
                Id::NoteText,
                Box::new(NoteText::new(text, shift_enter)),
                vec![],
            )?;
        }
        Some(p @ (Popup::ConfirmDelete | Popup::Gitignore)) => {
            for (attr, value) in confirm::attrs(p) {
                app.attr(&Id::Confirm, attr, value)?;
            }
        }
        Some(p @ (Popup::Finder | Popup::Search | Popup::FindInFile | Popup::GotoLine)) => {
            let kind = model.picker.as_ref().map(|p| p.kind);
            let title = match p {
                Popup::GotoLine => Title::from(" Go to line ").alignment(HorizontalAlignment::Left),
                _ => finder_popup::title_attr(kind),
            };
            app.attr(
                &Id::QueryInput,
                Attribute::Value,
                AttrValue::String(String::new()),
            )?;
            app.attr(&Id::QueryInput, Attribute::Title, AttrValue::Title(title))?;
        }
        _ => {}
    }
    if let Some(results) = model.picker_props() {
        app.attr(&Id::PickerResults, Attribute::Value, any(results))?;
    }
    if let Some(view) = model.note_view_props() {
        app.attr(&Id::NoteViewer, Attribute::Value, any(view))?;
    }
    if let Some(header) = model.note_header() {
        app.attr(&Id::NoteHeader, Attribute::Value, any(header))?;
    }
    app.attr(&Id::CodeView, Attribute::Value, any(model.code_props()))?;
    let tree_shown = model.view == View::Files && model.popup.is_none();
    app.attr(
        &Id::FileTree,
        Attribute::Custom(VISIBLE),
        AttrValue::Flag(tree_shown),
    )?;
    let marked: BTreeSet<String> = model
        .active_session()
        .map(|s| {
            s.notes
                .iter()
                .map(|n| n.file.display().to_string())
                .collect()
        })
        .unwrap_or_default();
    let marked = marked.into_iter().map(PropValue::Str).collect();
    app.attr(
        &Id::FileTree,
        Attribute::Custom(MARKED),
        AttrValue::Payload(PropPayload::Vec(marked)),
    )?;
    app.attr(
        &Id::SessionView,
        Attribute::Value,
        any(model.session_props()),
    )?;
    app.attr(
        &Id::FileTree,
        Attribute::Borders,
        AttrValue::Borders(borders(model.active_id() == Id::FileTree)),
    )?;
    let session = model
        .active_session()
        .map(|s| (s.name.as_str(), s.notes.len()));
    let status = status_bar::spans(model.view, session, model.location());
    app.attr(&Id::StatusBar, Attribute::Text, status_bar::attr(status))?;
    app.attr(
        &Id::DiffFiles,
        Attribute::Value,
        any(model.diff_list_props()),
    )?;
    if let Some(t) = &model.toast {
        app.attr(
            &Id::Toast,
            Attribute::Text,
            AttrValue::Text(Text::from(t.text.clone())),
        )?;
        app.attr(
            &Id::Toast,
            Attribute::Borders,
            AttrValue::Borders(toast::borders(t.kind)),
        )?;
    }
    app.active(&model.active_id())?;
    Ok(())
}

fn draw(app: &mut App, model: &Model, f: &mut Frame) {
    let screen = f.area();
    let areas = layout(screen);
    match model.view {
        View::Files => {
            app.view(&Id::FileTree, f, areas.tree);
            app.view(&Id::CodeView, f, areas.code);
        }
        View::Session => app.view(&Id::SessionView, f, areas.main),
        View::Diff => {
            app.view(&Id::DiffFiles, f, areas.tree);
            app.view(&Id::CodeView, f, areas.code);
        }
    }
    app.view(&Id::StatusBar, f, areas.status);
    let popup = match model.popup {
        Some(Popup::Help) => Some((Id::Help, help_popup::area(screen))),
        Some(Popup::NewSession) => Some((Id::NameInput, centered(screen, 44.min(screen.width), 3))),
        Some(Popup::SessionPicker) => {
            let height =
                (model.sessions.len() as u16 + 2).clamp(3, screen.height.saturating_sub(4));
            Some((
                Id::SessionPicker,
                centered(screen, 44.min(screen.width), height),
            ))
        }
        Some(p @ (Popup::ConfirmDelete | Popup::Gitignore)) => {
            Some((Id::Confirm, confirm::area(screen, p)))
        }
        Some(Popup::FindInFile | Popup::GotoLine) => {
            Some((Id::QueryInput, finder_popup::find_area(areas.code)))
        }
        Some(Popup::ViewNote) => model.note_view_props().map(|v| {
            (
                Id::NoteViewer,
                note_popup::viewer_area(screen, v.rows.len()),
            )
        }),
        Some(Popup::Note | Popup::Finder | Popup::Search) | None => None,
    };
    if let Some((id, area)) = popup {
        f.render_widget(Clear, area);
        app.view(&id, f, area);
    }
    if let Some(Popup::Finder | Popup::Search) = model.popup {
        let (input, results) = finder_popup::areas(screen);
        f.render_widget(Clear, input.union(results));
        app.view(&Id::QueryInput, f, input);
        app.view(&Id::PickerResults, f, results);
    }
    if let (Some(Popup::Note), Some(header)) = (model.popup, model.note_header()) {
        let (head, text) = note_popup::areas(screen, header.rows.len());
        f.render_widget(Clear, head.union(text));
        app.view(&Id::NoteHeader, f, head);
        app.view(&Id::NoteText, f, text);
    }
    if let Some(t) = &model.toast {
        let area = toast::area(areas.main, &t.text);
        f.render_widget(Clear, area);
        app.view(&Id::Toast, f, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Motion;
    use crate::highlight::load_theme;
    use crate::store::MemStore;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::terminal::TestTerminalAdapter;
    use tuirealm::testing::buffer_to_string;

    fn frame(model: &mut Model, opened: Option<Popup>) -> String {
        let (w, h) = model.size;
        let mut app = mount(&model.root, EventListenerCfg::default()).unwrap();
        let mut term = TestTerminalAdapter::new(Size::new(w, h)).unwrap();
        sync(model, &mut app, opened, false).unwrap();
        let done = term.draw(|f| draw(&mut app, model, f)).unwrap();
        buffer_to_string(done.buffer)
    }

    fn model(root: &Path) -> Model {
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(root.into(), theme, Box::new(MemStore::default())).unwrap();
        m.update(Msg::Resize(80, 14));
        m
    }

    #[test]
    fn files_view_frames() {
        let repo = crate::testutil::fixture_repo();
        let mut m = model(repo.path());
        insta::assert_snapshot!("frame_empty", frame(&mut m, None));

        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::Move(Motion::Down));
        m.update(Msg::Move(Motion::LineEnd));
        insta::assert_snapshot!("frame_file", frame(&mut m, None));

        m.update(Msg::OpenPopup(Popup::Help));
        insta::assert_snapshot!("frame_help", frame(&mut m, None));
    }

    #[test]
    fn diff_frames() {
        let repo = crate::testutil::fixture_repo();
        std::fs::write(
            repo.path().join("src/matcher.rs"),
            "pub fn matches() -> Vec<String> {\n    Vec::new()\n}\n",
        )
        .unwrap();
        let mut m = model(repo.path()).with_git(None);
        insta::assert_snapshot!(
            "frame_gitignore_offer",
            frame(&mut m, Some(Popup::Gitignore))
        );
        m.update(Msg::Confirm(false));
        m.update(Msg::Resize(80, 14));
        insta::assert_snapshot!("frame_diff_list", frame(&mut m, None));
        m.update(Msg::DiffMove(1));
        m.update(Msg::ToggleFocus);
        m.update(Msg::NewSession("S".into()));
        m.update(Msg::DismissToast);
        m.update(Msg::AddNote);
        m.update(Msg::SaveNote("note on an added line".into()));
        m.update(Msg::DismissToast);
        insta::assert_snapshot!("frame_diff_note", frame(&mut m, None));
    }

    #[test]
    fn session_frames() {
        let repo = crate::testutil::fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::ShowView(View::Session));
        insta::assert_snapshot!("frame_no_session", frame(&mut m, None));

        m.update(Msg::OpenPopup(Popup::NewSession));
        insta::assert_snapshot!("frame_new_session", frame(&mut m, Some(Popup::NewSession)));

        m.update(Msg::NewSession("Perf".into()));
        m.update(Msg::NewSession("Docs".into()));
        m.update(Msg::OpenPopup(Popup::SessionPicker));
        insta::assert_snapshot!("frame_picker", frame(&mut m, Some(Popup::SessionPicker)));

        m.update(Msg::PickSession(0));
        insta::assert_snapshot!("frame_session_empty_with_toast", frame(&mut m, None));
    }

    #[test]
    fn note_frames() {
        let repo = crate::testutil::fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::NewSession("Perf".into()));
        m.update(Msg::DismissToast);
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        for msg in [
            Msg::Move(Motion::Down),
            Msg::Move(Motion::Down),
            Msg::ToggleSelection(crate::app::SelectionKind::Line),
            Msg::Move(Motion::Down),
            Msg::Move(Motion::Down),
            Msg::AddNote,
        ] {
            m.update(msg);
        }
        insta::assert_snapshot!("frame_note_popup", frame(&mut m, Some(Popup::Note)));

        m.update(Msg::SaveNote("use an iterator chain".into()));
        m.update(Msg::DismissToast);
        insta::assert_snapshot!("frame_gutter_markers", frame(&mut m, None));

        m.update(Msg::ViewNote);
        insta::assert_snapshot!("frame_view_note", frame(&mut m, None));
        m.update(Msg::ClosePopup);

        m.update(Msg::ShowView(View::Session));
        m.update(Msg::AskDeleteNote);
        insta::assert_snapshot!("frame_confirm_delete", frame(&mut m, None));
    }

    #[test]
    fn search_frames() {
        let repo = crate::testutil::fixture_repo();
        let mut m = model(repo.path());
        m.update(Msg::OpenPopup(Popup::Finder));
        m.update(Msg::QueryChanged("web".into()));
        insta::assert_snapshot!("frame_finder", frame(&mut m, Some(Popup::Finder)));

        m.update(Msg::ClosePopup);
        m.update(Msg::OpenPopup(Popup::Search));
        m.update(Msg::QueryEnter("item".into()));
        m.update(Msg::PickerMove(2));
        insta::assert_snapshot!("frame_search", frame(&mut m, Some(Popup::Search)));

        m.update(Msg::ClosePopup);
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::OpenPopup(Popup::FindInFile));
        insta::assert_snapshot!("frame_find_in_file", frame(&mut m, Some(Popup::FindInFile)));
    }
}

/// `cargo test screenshots -- --ignored` redraws the screenshots in
/// `docs/site/img/` (README and docs site).
#[cfg(test)]
mod screenshots {
    use super::*;
    use crate::app::{Motion, SelectionKind};
    use crate::highlight::load_theme;
    use crate::store::MemStore;
    use std::fmt::Write as _;
    use tuirealm::ratatui::buffer::Buffer;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::ratatui::style::{Color, Modifier};
    use tuirealm::terminal::TestTerminalAdapter;

    const SIZE: (u16, u16) = (110, 26);
    const BG: &str = "#2b303b";
    const FG: &str = "#c0c5ce";

    fn hex(c: Color, fallback: &str) -> String {
        let named = match c {
            Color::Rgb(r, g, b) => return format!("#{r:02x}{g:02x}{b:02x}"),
            Color::Reset => fallback,
            Color::Black => "#1c1f26",
            Color::Red | Color::LightRed => "#bf616a",
            Color::Green | Color::LightGreen => "#a3be8c",
            Color::Yellow | Color::LightYellow => "#ebcb8b",
            Color::Blue | Color::LightBlue => "#8fa1b3",
            Color::Magenta | Color::LightMagenta => "#b48ead",
            Color::Cyan | Color::LightCyan => "#96b5b4",
            Color::Gray | Color::White => "#dfe1e8",
            Color::DarkGray => "#65737e",
            Color::Indexed(24) => "#2d4f67",
            Color::Indexed(_) => "#343d46",
        };
        named.to_string()
    }

    fn svg(buf: &Buffer) -> String {
        let (cw, ch) = (8.4, 18.0);
        let area = buf.area;
        let (w, h) = (
            area.width as f64 * cw + 24.0,
            area.height as f64 * ch + 24.0,
        );
        let mut out = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" font-family=\"Menlo, Consolas, monospace\" font-size=\"14\">\n<rect width=\"100%\" height=\"100%\" rx=\"8\" fill=\"{BG}\"/>\n"
        );
        for y in 0..area.height {
            for x in 0..area.width {
                let cell = &buf[(x, y)];
                let mut fg = hex(cell.fg, FG);
                let mut bg = hex(cell.bg, BG);
                if cell.modifier.contains(Modifier::REVERSED) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let (px, py) = (12.0 + x as f64 * cw, 12.0 + y as f64 * ch);
                if bg != BG {
                    let _ = writeln!(
                        out,
                        "<rect x=\"{px}\" y=\"{py}\" width=\"{}\" height=\"{ch}\" fill=\"{bg}\"/>",
                        cw + 0.4
                    );
                }
                let sym = cell.symbol();
                if sym.trim().is_empty() {
                    continue;
                }
                let text = sym
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                let bold = if cell.modifier.contains(Modifier::BOLD) {
                    " font-weight=\"bold\""
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "<text x=\"{px}\" y=\"{}\" fill=\"{fg}\"{bold}>{text}</text>",
                    py + 14.0
                );
            }
        }
        out + "</svg>\n"
    }

    fn shot(model: &mut Model, opened: Option<Popup>, name: &str) {
        let mut app = mount(&model.root, EventListenerCfg::default()).unwrap();
        let mut term = TestTerminalAdapter::new(Size::new(SIZE.0, SIZE.1)).unwrap();
        sync(model, &mut app, opened, true).unwrap();
        let done = term.draw(|f| draw(&mut app, model, f)).unwrap();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/site/img");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{name}.svg")), svg(done.buffer)).unwrap();
    }

    #[test]
    #[ignore]
    fn screenshots() {
        let repo = crate::testutil::fixture_repo();
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(repo.path().into(), theme, Box::new(MemStore::default()))
            .unwrap()
            .with_git(None);
        m.update(Msg::Confirm(false));
        m.update(Msg::Resize(SIZE.0, SIZE.1));
        m.update(Msg::NewSession("Search speedup".into()));
        m.update(Msg::ShowView(View::Files));
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        let note = |m: &mut Model, msgs: &[Msg], text: &str| {
            msgs.iter().for_each(|msg| m.update(msg.clone()));
            m.update(Msg::AddNote);
            m.update(Msg::SaveNote(text.into()));
        };
        let down = Msg::Move(Motion::Down);
        let line = Msg::ToggleSelection(SelectionKind::Line);
        note(
            &mut m,
            &[
                down.clone(),
                down.clone(),
                line.clone(),
                down.clone(),
                down.clone(),
                down.clone(),
                down.clone(),
            ],
            "replace the loop with an iterator chain:\nitems.iter().filter(..).cloned().collect()",
        );
        m.update(Msg::Move(Motion::Bottom));
        m.update(Msg::Move(Motion::Up));
        note(&mut m, &[], "return early when query is empty");
        m.update(Msg::OpenFile("scripts/score.py".into()));
        note(&mut m, std::slice::from_ref(&down), "use math.fsum here");
        m.update(Msg::DismissToast);

        // Files view: a selection on its way to becoming a note.
        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::Move(Motion::Down));
        m.update(line.clone());
        m.update(Msg::Move(Motion::Down));
        shot(&mut m, None, "files");
        m.update(Msg::AddNote);
        shot(&mut m, Some(Popup::Note), "note");
        m.update(Msg::ClosePopup);
        m.update(Msg::ClearSelection);

        m.update(Msg::ShowView(View::Diff));
        m.update(Msg::ToggleFocus);
        m.update(Msg::ToggleSelection(SelectionKind::Line));
        m.update(Msg::Move(Motion::Down));
        m.update(Msg::AddNote);
        m.update(Msg::SaveNote("read this from the build env instead".into()));
        m.update(Msg::DismissToast);
        shot(&mut m, None, "diff");

        m.update(Msg::ShowView(View::Session));
        m.update(Msg::NoteDown);
        shot(&mut m, None, "session");

        m.update(Msg::NewSession("Docs pass".into()));
        m.update(Msg::DismissToast);
        m.update(Msg::OpenPopup(Popup::SessionPicker));
        shot(&mut m, Some(Popup::SessionPicker), "picker");
        m.update(Msg::PickSession(0));

        m.update(Msg::ShowView(View::Files));
        m.update(Msg::OpenPopup(Popup::Finder));
        m.update(Msg::QueryChanged("app".into()));
        shot(&mut m, Some(Popup::Finder), "finder");
        m.update(Msg::ClosePopup);

        m.update(Msg::OpenPopup(Popup::Search));
        m.update(Msg::QueryEnter("item".into()));
        m.update(Msg::PickerMove(1));
        shot(&mut m, Some(Popup::Search), "search");
        m.update(Msg::ClosePopup);
    }
}
