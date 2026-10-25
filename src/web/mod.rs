//! `tackline serve`: the same notes and sessions in a browser, on localhost.
//!
//! Pages are rendered on the server (askama) and swapped in by htmx; every
//! change redirects back to the page it came from. The data is the same
//! `.tackline/` the terminal app uses, read fresh on every request.

mod api;
mod html;
mod pages;

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use two_face::re_exports::syntect::highlighting::Theme;

/// What every handler needs.
#[derive(Clone)]
pub struct Ctx {
    pub root: PathBuf,
    pub theme: Arc<Theme>,
    pub base: Option<String>,
    pub git: bool,
    /// Secret in the opened URL, then in a cookie: only that browser gets in.
    pub token: String,
    pub port: u16,
}

const COOKIE: &str = "tackline_token";

/// Start the server on 127.0.0.1:`port` and (unless `open` is false) open it in
/// the browser. Runs until interrupted.
pub fn serve(
    ctx_root: PathBuf,
    theme: Theme,
    base: Option<String>,
    port: u16,
    open: bool,
) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        let port = listener.local_addr()?.port();
        let ctx = Ctx {
            git: crate::git::repo_root(&ctx_root).is_some(),
            root: ctx_root,
            theme: Arc::new(theme),
            base,
            token: token(),
            port,
        };
        let url = format!("http://127.0.0.1:{port}/?token={}", ctx.token);
        println!("tackline is running at {url}");
        println!("Press Ctrl+C to stop.");
        if open {
            open_browser(&url);
        }
        axum::serve(listener, router(ctx)).await?;
        Ok(())
    })
}

pub fn router(ctx: Ctx) -> Router {
    Router::new()
        .route("/", get(pages::files))
        .route("/code", get(pages::files))
        .route("/diff", get(pages::diff))
        .route("/session", get(pages::session))
        .route("/search", get(pages::search))
        .route("/prompt", get(pages::prompt))
        .route("/notes", post(pages::add_note))
        .route("/notes/{id}/edit", get(pages::edit_note))
        .route("/notes/{id}", post(pages::save_note))
        .route("/notes/{id}/delete", post(pages::delete_note))
        .route("/notes/{id}/move", post(pages::move_note))
        .route("/sessions", post(pages::new_session))
        .route("/sessions/active", post(pages::activate_session))
        .route("/api/sessions", get(api::sessions))
        .route("/api/sessions/{id}", get(api::session))
        .route("/api/sessions/{id}/prompt", get(api::prompt))
        .route("/api/notes", post(api::add_note))
        .route("/api/notes/{id}", axum::routing::delete(api::delete_note))
        .route("/static/{file}", get(static_file))
        .layer(middleware::from_fn_with_state(ctx.clone(), guard))
        .with_state(ctx)
}

/// Only this machine, and only the browser that opened the printed link:
/// - `Host` must be 127.0.0.1/localhost on our port (stops DNS rebinding).
/// - The token from the link becomes a cookie that every request must carry
///   (stops other websites from reading code or changing notes).
async fn guard(State(ctx): State<Ctx>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let allowed = [
        format!("127.0.0.1:{}", ctx.port),
        format!("localhost:{}", ctx.port),
    ];
    if !allowed.iter().any(|a| a == host) {
        return (StatusCode::FORBIDDEN, "Wrong host").into_response();
    }
    if cookie(headers, COOKIE).is_some_and(|t| t == ctx.token) {
        return next.run(request).await;
    }
    // First visit through the printed link: remember the token, drop it from the URL.
    let query_token = request
        .uri()
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("token=")));
    if request.uri().path() == "/" && query_token == Some(ctx.token.as_str()) {
        let cookie = format!("{COOKIE}={}; Path=/; HttpOnly; SameSite=Strict", ctx.token);
        return ([(header::SET_COOKIE, cookie)], Redirect::to("/")).into_response();
    }
    let hint = "Open the link that `tackline serve` printed (it contains the access token).";
    (StatusCode::FORBIDDEN, hint).into_response()
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(name)?.strip_prefix('='))
}

/// 160 random bits (two ULIDs' randomness), plenty for a local session.
fn token() -> String {
    format!("{}{}", ulid::Ulid::generate(), ulid::Ulid::generate()).to_lowercase()
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(windows) {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    // Best effort: the URL is printed anyway.
    let _ = std::process::Command::new(cmd.0).args(cmd.1).spawn();
}

async fn static_file(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    let (body, kind): (&'static [u8], &str) = match file.as_str() {
        "htmx.min.js" => (
            include_bytes!("../../web/static/htmx.min.js"),
            "text/javascript",
        ),
        "app.js" => (include_bytes!("../../web/static/app.js"), "text/javascript"),
        "app.css" => (include_bytes!("../../web/static/app.css"), "text/css"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    (
        [
            (header::CONTENT_TYPE, kind),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

/// `rel` as a path inside the repo, or `None` for anything that could leave
/// it: absolute paths, `..`, and `.git/`.
pub fn safe_path(root: &Path, rel: &str) -> Option<PathBuf> {
    let rel = Path::new(rel);
    let ok = rel.components().all(|c| matches!(c, Component::Normal(_)))
        && rel
            .components()
            .next()
            .is_some_and(|c| c.as_os_str() != ".git");
    if !ok {
        return None;
    }
    // Symlinks pointing outside the repo are refused too.
    let full = root.join(rel).canonicalize().ok()?;
    full.starts_with(root.canonicalize().ok()?)
        .then(|| rel.to_path_buf())
}

#[cfg(test)]
mod tests;
