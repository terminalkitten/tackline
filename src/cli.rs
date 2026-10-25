//! Command line: open the app, `export` a prompt, list `sessions`.

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::model::{Session, note_count};
use crate::store::{self, FsStore, Store};
use crate::{app, compile, git, highlight};

/// Annotate code with prompt instructions and compile them into one prompt
/// for a coding agent.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Diff against merge-base(HEAD, <rev>) instead of HEAD, e.g. upstream/main.
    #[arg(long)]
    base: Option<String>,
    /// Syntax theme (default: follows the terminal's light/dark hint).
    #[arg(long)]
    theme: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Print the compiled prompt (default: active session). Warnings go to stderr.
    Export {
        /// Session name, file slug or id.
        session: Option<String>,
    },
    /// List sessions with note counts; `*` marks the active one.
    Sessions,
    /// Open tackline in the browser (on this machine only).
    Serve {
        /// Port on 127.0.0.1; 0 picks a free one.
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Don't open the browser, just print the link.
        #[arg(long)]
        no_open: bool,
    },
}

/// The `tackline` (and `tack`) command.
pub fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let root = git::repo_root(&cwd).unwrap_or(cwd);
    let store = FsStore::new(&root);

    let theme = || {
        highlight::load_theme(cli.theme.as_deref()).with_context(|| {
            let names: Vec<_> = highlight::theme_names().collect();
            format!("unknown theme; available: {}", names.join(", "))
        })
    };
    match cli.command {
        Some(Command::Serve { port, no_open }) => {
            if let Some(rev) = &cli.base {
                git::diff(&root, Some(rev)).with_context(|| format!("--base {rev}"))?;
            }
            crate::web::serve(root, theme()?, cli.base.clone(), port, !no_open)?;
        }
        None => {
            let theme = theme()?;
            let mut model = app::Model::new(root.clone(), theme, Box::new(FsStore::new(&root)))?;
            if git::repo_root(&root).is_some() {
                if let Some(rev) = &cli.base {
                    git::diff(&root, Some(rev)).with_context(|| format!("--base {rev}"))?;
                }
                model = model.with_git(cli.base);
            } else if cli.base.is_some() {
                bail!("--base needs a git repository");
            }
            app::run(model)?;
        }
        Some(Command::Sessions) => {
            let active = store.load_state()?.active_session_id;
            for s in store.load_sessions()? {
                let mark = if Some(&s.id) == active.as_ref() {
                    '*'
                } else {
                    ' '
                };
                println!("{mark} {} ({})", s.name, note_count(s.notes.len()));
            }
        }
        Some(Command::Export { session }) => {
            let session = find_session(&store, session.as_deref())?;
            let (prompt, warnings) = compile::compile(&session, &root);
            for w in warnings {
                eprintln!("warning: {w}");
            }
            print!("{prompt}");
        }
    }
    Ok(())
}

fn find_session(store: &impl Store, query: Option<&str>) -> Result<Session> {
    let sessions = store.load_sessions()?;
    let found = match query {
        Some(q) => sessions
            .into_iter()
            .find(|s| s.name == q || store::slug(&s.name) == q || s.id == q)
            .with_context(|| format!("no session named {q:?}"))?,
        None => {
            let id = store
                .load_state()?
                .active_session_id
                .context("no active session; pass a session name")?;
            sessions
                .into_iter()
                .find(|s| s.id == id)
                .context("the active session no longer exists; pass a session name")?
        }
    };
    Ok(found)
}
