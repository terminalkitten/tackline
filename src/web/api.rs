//! JSON API on the same logic: sessions, notes (with where they are now) and
//! the compiled prompt.

use axum::Json;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::Ctx;
use super::pages::{AddNote, add_note_to};
use crate::anchor;
use crate::compile::compile;
use crate::model::{Session, Span};
use crate::store::{FsStore, Store};

#[derive(Serialize)]
struct SessionSummary {
    id: String,
    name: String,
    notes: usize,
    active: bool,
}

fn load(ctx: &Ctx) -> (FsStore, Vec<Session>, Option<String>) {
    let store = FsStore::new(&ctx.root);
    let sessions = store.load_sessions().unwrap_or_default();
    let active = store.load_state().ok().and_then(|s| s.active_session_id);
    (store, sessions, active)
}

fn not_found(what: &str) -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": format!("no {what}") })),
    )
        .into_response()
}

/// `GET /api/sessions`
pub async fn sessions(State(ctx): State<Ctx>) -> Response {
    let (_, sessions, active) = load(&ctx);
    let list: Vec<SessionSummary> = sessions
        .into_iter()
        .map(|s| SessionSummary {
            active: Some(&s.id) == active.as_ref(),
            notes: s.notes.len(),
            id: s.id,
            name: s.name,
        })
        .collect();
    Json(list).into_response()
}

#[derive(Serialize)]
struct NoteNow {
    id: String,
    file: String,
    /// Where the note is now (relocated), or the stored span when stale.
    span: Span,
    stale: bool,
    text: String,
}

/// `GET /api/sessions/{id}`: the session with each note located in the files
/// as they are now. `active` stands for the active session.
pub async fn session(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> Response {
    let (_, sessions, active) = load(&ctx);
    let id = if id == "active" {
        active.unwrap_or_default()
    } else {
        id
    };
    let Some(session) = sessions.into_iter().find(|s| s.id == id) else {
        return not_found("session");
    };
    let notes: Vec<NoteNow> = session
        .notes
        .iter()
        .map(|n| {
            let (_, span) = anchor::locate_on_disk(&ctx.root, n);
            NoteNow {
                id: n.id.clone(),
                file: n.file.display().to_string(),
                span: span.unwrap_or(n.span),
                stale: span.is_none(),
                text: n.text.clone(),
            }
        })
        .collect();
    Json(serde_json::json!({ "id": session.id, "name": session.name, "notes": notes }))
        .into_response()
}

/// `GET /api/sessions/{id}/prompt`: the compiled prompt and any warnings.
pub async fn prompt(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> Response {
    let (_, sessions, active) = load(&ctx);
    let id = if id == "active" {
        active.unwrap_or_default()
    } else {
        id
    };
    let Some(session) = sessions.iter().find(|s| s.id == id) else {
        return not_found("session");
    };
    let (prompt, warnings) = compile(session, &ctx.root);
    let warnings: Vec<String> = warnings.iter().map(|w| w.to_string()).collect();
    Json(serde_json::json!({ "prompt": prompt, "warnings": warnings })).into_response()
}

#[derive(Deserialize)]
pub struct NewNote {
    file: String,
    /// 1-based, inclusive.
    start_line: usize,
    end_line: usize,
    text: String,
    /// Session id; the active one when left out.
    session: Option<String>,
}

/// `POST /api/notes` `{file, start_line, end_line, text, session?}`
pub async fn add_note(State(ctx): State<Ctx>, Json(body): Json<NewNote>) -> Response {
    let form = AddNote {
        file: body.file,
        start: body.start_line,
        end: body.end_line,
        view: "code".into(),
        text: body.text,
        session: body.session.unwrap_or_default(),
        new_session: String::new(),
        back: String::new(),
    };
    match add_note_to(&ctx, &form) {
        Ok((session, _)) => {
            let (_, sessions, _) = load(&ctx);
            let s = &sessions[session];
            let note = s.notes.last().expect("just added");
            let body = serde_json::json!({ "id": note.id, "session": s.id, "span": note.span });
            (StatusCode::CREATED, Json(body)).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// `DELETE /api/notes/{id}`
pub async fn delete_note(State(ctx): State<Ctx>, UrlPath(id): UrlPath<String>) -> Response {
    let (store, mut sessions, _) = load(&ctx);
    for s in &mut sessions {
        if let Some(i) = s.notes.iter().position(|n| n.id == id) {
            s.notes.remove(i);
            let _ = store.save_session(s);
            return StatusCode::NO_CONTENT.into_response();
        }
    }
    not_found("note")
}
