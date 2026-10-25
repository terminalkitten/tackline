//! Terminal loop: tuirealm events → `Msg` → `Model::update` → props → draw.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tuirealm::application::{Application, PollStrategy};
use tuirealm::event::NoUserEvent;
use tuirealm::listener::EventListenerCfg;
use tuirealm::props::{AttrValue, Attribute, PropBound, PropPayload, Title};
use tuirealm::ratatui::Frame;
use tuirealm::ratatui::text::Text;
use tuirealm::ratatui::widgets::Clear;
use tuirealm::terminal::{CrosstermTerminalAdapter, TerminalAdapter};

use super::{Id, Model, Msg, layout};
use crate::components::code_view::CodeView;
use crate::components::file_tree::{FileTree, build_tree};
use crate::components::help_popup::{self, HelpPopup};
use crate::components::status_bar::{self, StatusBar};
use crate::components::{Placeholder, borders};
use crate::model::View;
use crate::search::files::walk;

type App = Application<Id, Msg, NoUserEvent>;

pub fn run(mut model: Model) -> Result<()> {
    let mut app = mount(
        &model.root,
        EventListenerCfg::default().crossterm_input_listener(Duration::from_millis(20), 3),
    )?;
    // `new` also installs a panic hook that leaves raw mode and the
    // alternate screen before the panic message prints.
    let mut term = CrosstermTerminalAdapter::new()?;
    term.enable_raw_mode()?;
    term.enter_alternate_screen()?;

    let mut dirty = true;
    while !model.quit {
        let size = term.raw().size()?;
        if (size.width, size.height) != model.size {
            model.update(Msg::Resize(size.width, size.height));
            dirty = true;
        }
        if dirty {
            sync(&mut model, &mut app)?;
            term.draw(|f| draw(&mut app, &model, f))?;
        }
        // Keep drawing while highlighting catches up with the visible rows.
        dirty = model.highlight_pending();
        let wait = if dirty { 1 } else { 50 };
        for msg in app.tick(PollStrategy::Once(Duration::from_millis(wait)))? {
            model.update(msg);
            dirty = true;
        }
    }
    term.restore()?;
    Ok(())
}

fn mount(root: &Path, listener: EventListenerCfg<NoUserEvent>) -> Result<App> {
    let mut app = Application::init(listener);
    let name = root
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let tree = build_tree(&name, &walk(root));
    app.mount(Id::FileTree, Box::new(FileTree::new(tree)), vec![])?;
    app.mount(Id::CodeView, Box::new(CodeView::default()), vec![])?;
    app.mount(Id::Placeholder, Box::new(Placeholder::default()), vec![])?;
    app.mount(Id::StatusBar, Box::new(StatusBar::default()), vec![])?;
    app.mount(Id::Help, Box::new(HelpPopup::default()), vec![])?;
    Ok(app)
}

/// Push the model's state into the components' props.
fn sync(model: &mut Model, app: &mut App) -> Result<()> {
    let code = model.code_props();
    app.attr(
        &Id::CodeView,
        Attribute::Value,
        AttrValue::Payload(PropPayload::Any(code.to_any_prop())),
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
    let (title, text) = match model.view {
        View::Files => ("", ""),
        View::Diff => (" Diff ", "The Diff view lands in milestone 6."),
        View::Session => (" Session ", "The Session view lands in milestone 3."),
    };
    app.attr(
        &Id::Placeholder,
        Attribute::Title,
        AttrValue::Title(Title::from(title)),
    )?;
    app.attr(
        &Id::Placeholder,
        Attribute::Text,
        AttrValue::Text(Text::from(text)),
    )?;
    app.active(&model.active_id())?;
    Ok(())
}

fn draw(app: &mut App, model: &Model, f: &mut Frame) {
    let areas = layout(f.area());
    match model.view {
        View::Files => {
            app.view(&Id::FileTree, f, areas.tree);
            app.view(&Id::CodeView, f, areas.code);
        }
        View::Diff | View::Session => app.view(&Id::Placeholder, f, areas.main),
    }
    app.view(&Id::StatusBar, f, areas.status);
    if model.help {
        let area = help_popup::area(f.area());
        f.render_widget(Clear, area);
        app.view(&Id::Help, f, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Motion;
    use crate::highlight::load_theme;
    use crate::model::State;
    use tuirealm::ratatui::layout::Size;
    use tuirealm::terminal::TestTerminalAdapter;
    use tuirealm::testing::buffer_to_string;

    fn frame(model: &mut Model) -> String {
        let (w, h) = model.size;
        let mut app = mount(&model.root, EventListenerCfg::default()).unwrap();
        let mut term = TestTerminalAdapter::new(Size::new(w, h)).unwrap();
        sync(model, &mut app).unwrap();
        let done = term.draw(|f| draw(&mut app, model, f)).unwrap();
        buffer_to_string(done.buffer)
    }

    #[test]
    fn files_view_frames() {
        let repo = crate::testutil::fixture_repo();
        let theme = load_theme(Some("base16-ocean.dark")).unwrap();
        let mut m = Model::new(repo.path().to_path_buf(), theme, vec![], State::default());
        m.update(Msg::Resize(80, 14));
        insta::assert_snapshot!("frame_empty", frame(&mut m));

        m.update(Msg::OpenFile("src/matcher.rs".into()));
        m.update(Msg::Move(Motion::Down));
        m.update(Msg::Move(Motion::LineEnd));
        insta::assert_snapshot!("frame_file", frame(&mut m));

        m.update(Msg::ToggleHelp);
        insta::assert_snapshot!("frame_help", frame(&mut m));

        m.update(Msg::CloseHelp);
        m.update(Msg::ShowView(View::Diff));
        insta::assert_snapshot!("frame_diff_placeholder", frame(&mut m));
    }
}
