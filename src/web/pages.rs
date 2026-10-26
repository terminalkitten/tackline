//! HTML pages and the form actions behind them. Each action redirects back to
//! the page it came from; htmx (boost) swaps the new page in.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use askama::Template;
use axum::extract::{Form, Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use serde::Deserialize;

use super::html::{self, escape};
use super::{Ctx, safe_path};
use crate::anchor::{self, capture};
use crate::compile::compile;
use crate::git::{self, Row, RowKind};
use crate::highlight::Highlighted;
use crate::model::{Note, Session, State as AppState, note_count};
use crate::search::files::{find, read_lines, walk};
use crate::search::grep::grep;
use crate::store::{FsStore, Store};

/// Lines highlighted per file; past this they're plain text.
// ponytail: highlighting is synchronous per request; fine for code files,
// raise or stream if huge files feel slow.
const HIGHLIGHT_LIMIT: usize = 20_000;
const SNIPPET_LINES: usize = 6;

// ---- shared ---------------------------------------------------------------

pub struct Nav {
    pub tab: &'static str,
    pub sessions: Vec<SessionItem>,
    /// This page's URL, for forms to come back to.
    pub here: String,
    pub msg: Option<String>,
    pub git: bool,
    /// `:root` CSS: the theme's background and text colors.
    pub theme_css: String,
}

pub struct SessionItem {
    pub id: String,
    pub name: String,
    pub count: String,
    pub active: bool,
}

struct Data {
    store: FsStore,
    sessions: Vec<Session>,
    state: AppState,
}

impl Data {
    fn load(ctx: &Ctx) -> Data {
        let store = FsStore::new(&ctx.root);
        Data {
            sessions: store.load_sessions().unwrap_or_default(),
            state: store.load_state().unwrap_or_default(),
            store,
        }
    }

    fn active(&self) -> Option<usize> {
        let id = self.state.active_session_id.as_ref()?;
        self.sessions.iter().position(|s| &s.id == id)
    }

    fn nav(&self, ctx: &Ctx, tab: &'static str, here: String, msg: Option<String>) -> Nav {
        let active = self.active();
        let color = |c: Option<two_face::re_exports::syntect::highlighting::Color>, d: &str| match c
        {
            Some(c) if c.a >= 2 => format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b),
            _ => d.to_string(),
        };
        let settings = &ctx.theme.settings;
        let theme_css = format!(
            "--code-bg:{};--code-fg:{};",
            color(settings.background, "#282c34"),
            color(settings.foreground, "#abb2bf")
        );
        Nav {
            tab,
            sessions: self
                .sessions
                .iter()
                .enumerate()
                .map(|(i, s)| SessionItem {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    count: note_count(s.notes.len()),
                    active: Some(i) == active,
                })
                .collect(),
            here,
            msg,
            git: ctx.git,
            theme_css,
        }
    }

    fn save(&self, i: usize) {
        let _ = self.store.save_session(&self.sessions[i]);
    }
}

fn render(t: impl Template) -> Response {
    match t.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// Percent-encode a query value.
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Back to `url` (a path on this site only), optionally with a message.
fn back(url: &str, msg: Option<&str>) -> Response {
    let url = if url.starts_with('/') && !url.starts_with("//") {
        url
    } else {
        "/"
    };
    // Drop an earlier message so they don't pile up.
    let url = &strip_param(url, "msg");
    let url = match msg {
        Some(m) => format!(
            "{url}{}msg={}",
            if url.contains('?') { '&' } else { '?' },
            enc(m)
        ),
        None => url.to_string(),
    };
    Redirect::to(&url).into_response()
}

fn with_line(url: &str, line: usize) -> String {
    let base = strip_param(url, "line");
    format!(
        "{base}{}line={line}",
        if base.contains('?') { '&' } else { '?' }
    )
}

/// `url` without the query parameter `name`.
fn strip_param(url: &str, name: &str) -> String {
    let Some((path, query)) = url.split_once('?') else {
        return url.to_string();
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|kv| kv.split('=').next() != Some(name))
        .collect();
    if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    }
}

// ---- code rows ------------------------------------------------------------

pub struct CodeRow {
    /// Selection index: the line number (files) or row index (diff).
    pub index: usize,
    pub num: String,
    pub old: String,
    pub sign: &'static str,
    pub kind: &'static str,
    pub html: String,
    pub marker: &'static str,
    pub noted: bool,
    pub stale: bool,
    pub note_id: String,
    pub excerpt: String,
    /// Notes that end on this row: shown in a row of their own below it.
    pub notes_after: Vec<NoteCard>,
}

/// A note shown inline under its last line, like a review comment.
pub struct NoteCard {
    pub id: String,
    pub text: String,
    /// `src/matcher.rs:L3-5`.
    pub location: String,
    pub stale: bool,
}

pub struct CodeView {
    pub path: String,
    pub rows: Vec<CodeRow>,
    pub placeholder: Option<String>,
    pub focus: usize,
    pub diff: bool,
    /// Notes of the active session shown in this file.
    pub notes: usize,
}

impl CodeView {
    fn empty(path: String, placeholder: String, focus: usize, diff: bool) -> CodeView {
        CodeView {
            path,
            rows: Vec::new(),
            placeholder: Some(placeholder),
            focus,
            diff,
            notes: 0,
        }
    }
}

fn highlighted(ctx: &Ctx, path: &Path, lines: &[String]) -> Vec<String> {
    let first = lines.first().map_or("", String::as_str);
    let mut hl = Highlighted::new(path, first, &ctx.theme);
    hl.ensure(lines, lines.len().min(HIGHLIGHT_LIMIT), &ctx.theme);
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| hl.lines.get(i).map_or_else(|| escape(l), html::line))
        .collect()
}

/// Gutter markers and note ids per row, from where the active session's notes
/// are in `text` now. `to_rows` maps a note's lines to rows. Returns how many
/// notes are shown.
fn marks(
    rows: &mut [CodeRow],
    session: Option<&Session>,
    path: &Path,
    text: &str,
    to_rows: impl Fn(u32, u32) -> Option<(usize, usize)>,
) -> usize {
    let Some(session) = session else { return 0 };
    let mut shown = 0;
    for l in anchor::locate(&session.notes, path, text) {
        let Some((first, last)) = to_rows(l.span.start_line, l.span.end_line) else {
            continue;
        };
        let note = &session.notes[l.note];
        let excerpt: String = note.text.split_whitespace().collect::<Vec<_>>().join(" ");
        for (i, row) in rows.iter_mut().enumerate().take(last + 1).skip(first) {
            let end = i == first || i == last;
            if !row.noted || end {
                row.marker = if l.stale && i == first {
                    "⚠"
                } else if end {
                    "●"
                } else {
                    "│"
                };
            }
            row.noted = true;
            row.stale |= l.stale;
            row.note_id = note.id.clone();
            row.excerpt = excerpt.clone();
        }
        if let Some(row) = rows.get_mut(last) {
            row.notes_after.push(NoteCard {
                id: note.id.clone(),
                text: note.text.clone(),
                location: format!("{}:{}", path.display(), l.span.ui_label()),
                stale: l.stale,
            });
            shown += 1;
        }
    }
    shown
}

fn file_view(ctx: &Ctx, data: &Data, rel: &Path, focus: usize) -> CodeView {
    let path = rel.display().to_string();
    let lines = match read_lines(&ctx.root.join(rel)) {
        Ok(lines) => lines,
        Err(e) => return CodeView::empty(path, e, focus, false),
    };
    let mut rows: Vec<CodeRow> = highlighted(ctx, rel, &lines)
        .into_iter()
        .enumerate()
        .map(|(i, html)| CodeRow {
            index: i + 1,
            num: (i + 1).to_string(),
            old: String::new(),
            sign: "",
            kind: "",
            html,
            marker: "",
            noted: false,
            stale: false,
            note_id: String::new(),
            excerpt: String::new(),
            notes_after: Vec::new(),
        })
        .collect();
    let session = data.active().map(|i| &data.sessions[i]);
    let notes = marks(&mut rows, session, rel, &lines.join("\n"), |s, e| {
        Some((s as usize - 1, e as usize - 1))
    });
    CodeView {
        path,
        rows,
        placeholder: None,
        focus,
        diff: false,
        notes,
    }
}

fn diff_view(ctx: &Ctx, data: &Data, file: &git::FileDiff, focus: usize) -> CodeView {
    let path = file.path.display().to_string();
    if file.binary || file.rows.is_empty() {
        let msg = if file.binary {
            "Binary file changed"
        } else {
            "No line changes (an empty file, or only its mode changed)"
        };
        return CodeView::empty(path, msg.into(), focus, true);
    }
    let texts: Vec<String> = file.rows.iter().map(|r| r.text.clone()).collect();
    let html = highlighted(ctx, &file.path, &texts);
    let num = |n: Option<u32>| n.map_or(String::new(), |n| n.to_string());
    let mut rows: Vec<CodeRow> = file
        .rows
        .iter()
        .zip(html)
        .enumerate()
        .map(|(i, (r, html)): (usize, (&Row, String))| CodeRow {
            index: i,
            num: num(r.new),
            old: num(r.old),
            sign: match r.kind {
                RowKind::Added => "+",
                RowKind::Removed => "-",
                _ => " ",
            },
            kind: match r.kind {
                RowKind::Added => "add",
                RowKind::Removed => "del",
                RowKind::Hunk => "hunk",
                RowKind::Context => "",
            },
            html: if r.kind == RowKind::Hunk {
                escape(&r.text)
            } else {
                html
            },
            marker: "",
            noted: false,
            stale: false,
            note_id: String::new(),
            excerpt: String::new(),
            notes_after: Vec::new(),
        })
        .collect();
    let text = std::fs::read_to_string(ctx.root.join(&file.path)).unwrap_or_default();
    let session = data.active().map(|i| &data.sessions[i]);
    let notes = marks(&mut rows, session, &file.path, &text, |s, e| {
        git::rows_showing(&file.rows, s, e)
    });
    CodeView {
        path,
        rows,
        placeholder: None,
        focus,
        diff: true,
        notes,
    }
}

// ---- file tree ------------------------------------------------------------

fn tree_html(ctx: &Ctx, data: &Data, current: Option<&Path>) -> String {
    let entries = walk(&ctx.root);
    let noted: Vec<PathBuf> = data
        .active()
        .map(|i| {
            data.sessions[i]
                .notes
                .iter()
                .map(|n| n.file.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut children: BTreeMap<PathBuf, Vec<(PathBuf, bool)>> = BTreeMap::new();
    for (p, d) in &entries {
        children
            .entry(p.parent().unwrap_or(Path::new("")).to_path_buf())
            .or_default()
            .push((p.clone(), *d));
    }
    fn node(
        dir: &Path,
        children: &BTreeMap<PathBuf, Vec<(PathBuf, bool)>>,
        noted: &[PathBuf],
        current: Option<&Path>,
        out: &mut String,
    ) {
        let mut kids = children.get(dir).cloned().unwrap_or_default();
        kids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        for (p, is_dir) in kids {
            let name = escape(
                &p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            );
            if is_dir {
                let open = current.is_some_and(|c| c.starts_with(&p));
                out.push_str(&format!(
                    "<details{}><summary>{name}/</summary>",
                    if open { " open" } else { "" }
                ));
                node(&p, children, noted, current, out);
                out.push_str("</details>");
            } else {
                let path = p.display().to_string();
                let class = if current == Some(p.as_path()) {
                    " class=\"current\""
                } else {
                    ""
                };
                let mark = if noted.contains(&p) {
                    " <span class=\"dot\">●</span>"
                } else {
                    ""
                };
                out.push_str(&format!(
                    "<a href=\"/code?path={}\"{class}>{name}{mark}</a>",
                    enc(&path)
                ));
            }
        }
    }
    let mut out = String::new();
    node(Path::new(""), &children, &noted, current, &mut out);
    out
}

// ---- pages ----------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct PageQuery {
    path: Option<String>,
    line: Option<usize>,
    msg: Option<String>,
    q: Option<String>,
}

fn here(base: &str, path: Option<&str>) -> String {
    match path {
        Some(p) => format!("{base}?path={}", enc(p)),
        None => base.to_string(),
    }
}

#[derive(Template)]
#[template(path = "files.html")]
struct FilesPage {
    nav: Nav,
    tree: String,
    code: Option<CodeView>,
}

pub async fn files(State(ctx): State<Ctx>, Query(q): Query<PageQuery>) -> Response {
    let data = Data::load(&ctx);
    let rel = q.path.as_deref().and_then(|p| safe_path(&ctx.root, p));
    if q.path.is_some() && rel.is_none() {
        return (StatusCode::NOT_FOUND, "No such file in this repository").into_response();
    }
    let code = rel
        .as_deref()
        .map(|r| file_view(&ctx, &data, r, q.line.unwrap_or(0)));
    let nav = data.nav(&ctx, "files", here("/code", q.path.as_deref()), q.msg);
    render(FilesPage {
        nav,
        tree: tree_html(&ctx, &data, rel.as_deref()),
        code,
    })
}

pub struct DiffItem {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub selected: bool,
}

#[derive(Template)]
#[template(path = "diff.html")]
struct DiffPage {
    nav: Nav,
    against: String,
    files: Vec<DiffItem>,
    code: Option<CodeView>,
    error: Option<String>,
}

pub async fn diff(State(ctx): State<Ctx>, Query(q): Query<PageQuery>) -> Response {
    let data = Data::load(&ctx);
    let nav = data.nav(&ctx, "diff", here("/diff", q.path.as_deref()), q.msg);
    let against = ctx.base.clone().unwrap_or_else(|| "HEAD".into());
    let (files, error) = if ctx.git {
        match git::diff(&ctx.root, ctx.base.as_deref()) {
            Ok(f) => (f, None),
            Err(e) => (
                Vec::new(),
                Some(format!("git diff failed: {}", e.message())),
            ),
        }
    } else {
        (
            Vec::new(),
            Some("Not a git repository: no Diff view".into()),
        )
    };
    let selected = q
        .path
        .as_deref()
        .and_then(|p| files.iter().position(|f| f.path == Path::new(p)))
        .unwrap_or(0);
    let code = files
        .get(selected)
        .map(|f| diff_view(&ctx, &data, f, q.line.unwrap_or(0)));
    let items = files
        .iter()
        .enumerate()
        .map(|(i, f)| DiffItem {
            path: f.path.display().to_string(),
            added: f.added,
            removed: f.removed,
            selected: i == selected,
        })
        .collect();
    render(DiffPage {
        nav,
        against,
        files: items,
        code,
        error,
    })
}

pub struct NoteItem {
    pub id: String,
    pub location: String,
    pub snippet: Vec<String>,
    pub more: usize,
    pub text: String,
    pub stale: bool,
    pub jump: Option<String>,
}

#[derive(Template)]
#[template(path = "session.html")]
struct SessionPage {
    nav: Nav,
    id: String,
    name: Option<String>,
    notes: Vec<NoteItem>,
    prompt: String,
    warnings: Vec<String>,
}

pub async fn session(State(ctx): State<Ctx>, Query(q): Query<PageQuery>) -> Response {
    let data = Data::load(&ctx);
    let nav = data.nav(&ctx, "session", "/session".into(), q.msg);
    let Some(i) = data.active() else {
        return render(SessionPage {
            nav,
            id: String::new(),
            name: None,
            notes: Vec::new(),
            prompt: String::new(),
            warnings: Vec::new(),
        });
    };
    let session = &data.sessions[i];
    let notes = session
        .notes
        .iter()
        .map(|note| {
            let (text, span) = anchor::locate_on_disk(&ctx.root, note);
            let lines: Vec<String> = match (span, &text) {
                (Some(s), Some(t)) => t
                    .lines()
                    .skip(s.start_line as usize - 1)
                    .take((s.end_line - s.start_line) as usize + 1)
                    .map(str::to_string)
                    .collect(),
                _ => note.anchor.selected.lines().map(str::to_string).collect(),
            };
            let shown = &lines[..lines.len().min(SNIPPET_LINES)];
            NoteItem {
                id: note.id.clone(),
                location: format!(
                    "{}:{}",
                    note.file.display(),
                    span.unwrap_or(note.span).ui_label()
                ),
                snippet: highlighted(&ctx, &note.file, shown),
                more: lines.len().saturating_sub(SNIPPET_LINES),
                text: note.text.clone(),
                stale: span.is_none(),
                jump: span.map(|s| {
                    format!(
                        "/code?path={}&line={}",
                        enc(&note.file.display().to_string()),
                        s.start_line
                    )
                }),
            }
        })
        .collect();
    let (prompt, warnings) = compile(session, &ctx.root);
    render(SessionPage {
        nav,
        id: session.id.clone(),
        name: Some(session.name.clone()),
        notes,
        prompt,
        warnings: warnings.iter().map(|w| w.to_string()).collect(),
    })
}

pub struct FileHit {
    pub path: String,
}

pub struct CodeHit {
    pub path: String,
    pub line: usize,
    pub text: String,
}

#[derive(Template)]
#[template(path = "search.html")]
struct SearchPage {
    nav: Nav,
    q: String,
    files: Vec<FileHit>,
    hits: Vec<CodeHit>,
}

pub async fn search(State(ctx): State<Ctx>, Query(q): Query<PageQuery>) -> Response {
    let data = Data::load(&ctx);
    let query = q.q.unwrap_or_default();
    let nav = data.nav(&ctx, "search", format!("/search?q={}", enc(&query)), q.msg);
    let (files, hits) = if query.trim().is_empty() {
        (Vec::new(), Vec::new())
    } else {
        let all: Vec<String> = walk(&ctx.root)
            .into_iter()
            .filter(|(_, d)| !d)
            .map(|(p, _)| p.display().to_string())
            .collect();
        let files = find(&all, &query, 30)
            .into_iter()
            .map(|path| FileHit { path })
            .collect();
        let hits = grep(&ctx.root, &query, 200)
            .into_iter()
            .map(|h| CodeHit {
                path: h.path.display().to_string(),
                line: h.line,
                text: h.text,
            })
            .collect();
        (files, hits)
    };
    render(SearchPage {
        nav,
        q: query,
        files,
        hits,
    })
}

/// The active session's compiled prompt, for the Copy button.
pub async fn prompt(State(ctx): State<Ctx>) -> Response {
    let data = Data::load(&ctx);
    match data.active() {
        Some(i) => {
            let (text, _) = compile(&data.sessions[i], &ctx.root);
            ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], text).into_response()
        }
        None => (StatusCode::NOT_FOUND, "No active session").into_response(),
    }
}

// ---- actions --------------------------------------------------------------

#[derive(Deserialize)]
pub struct AddNote {
    pub file: String,
    /// Line numbers (files) or row indices (diff), inclusive.
    pub start: usize,
    pub end: usize,
    /// `code` or `diff`.
    pub view: String,
    pub text: String,
    /// Session id; empty for the active one.
    #[serde(default)]
    pub session: String,
    /// Name for a new session to put the note in.
    #[serde(default)]
    pub new_session: String,
    #[serde(rename = "return", default)]
    pub back: String,
}

/// Add a note from a selection: the same anchor capture as the terminal app.
pub fn add_note_to(ctx: &Ctx, form: &AddNote) -> Result<(usize, u32), String> {
    let rel = safe_path(&ctx.root, &form.file).ok_or("No such file in this repository")?;
    let text = form.text.trim();
    if text.is_empty() {
        return Err("The note needs some text".into());
    }
    let (start, end) = (form.start.min(form.end), form.start.max(form.end));
    let (first, last) = if form.view == "diff" {
        let files =
            git::diff(&ctx.root, ctx.base.as_deref()).map_err(|e| e.message().to_string())?;
        let file = files
            .iter()
            .find(|f| f.path == rel)
            .ok_or("That file isn't in the diff any more")?;
        let rows = file
            .rows
            .get(start..=end)
            .ok_or("That part of the diff changed: reload")?;
        git::new_lines(rows)
            .ok_or("Removed lines and hunk headers can't have notes: select new-side lines")?
    } else {
        (start as u32, end as u32)
    };
    let span = crate::model::Span::lines(first, last);
    let source = std::fs::read_to_string(ctx.root.join(&rel)).map_err(|e| e.to_string())?;
    let anchor = capture(span, &source).ok_or("Those lines aren't in the file any more: reload")?;
    let mut data = Data::load(ctx);
    let target = match data.sessions.iter().position(|s| s.id == form.session) {
        Some(i) => i,
        None if !form.new_session.trim().is_empty() || data.sessions.is_empty() => {
            let name = form.new_session.trim();
            let session = Session::new(if name.is_empty() { "Notes" } else { name });
            data.state.active_session_id = Some(session.id.clone());
            let _ = data.store.save_state(&data.state);
            data.sessions.push(session);
            data.sessions.len() - 1
        }
        None => data.active().unwrap_or(0),
    };
    data.sessions[target]
        .notes
        .push(Note::new(rel, span, anchor, text));
    data.save(target);
    Ok((target, first))
}

pub async fn add_note(State(ctx): State<Ctx>, Form(form): Form<AddNote>) -> Response {
    match add_note_to(&ctx, &form) {
        Ok(_) => back(
            &with_line(&form.back, form.start.min(form.end)),
            Some("Note added"),
        ),
        Err(e) => back(&form.back, Some(&e)),
    }
}

/// (session, note) of the note with this id.
fn find_note(sessions: &[Session], id: &str) -> Option<(usize, usize)> {
    sessions
        .iter()
        .enumerate()
        .find_map(|(s, session)| Some((s, session.notes.iter().position(|n| n.id == id)?)))
}

#[derive(Template)]
#[template(path = "note.html")]
struct NotePage {
    nav: Nav,
    id: String,
    location: String,
    code: Vec<String>,
    text: String,
    session_id: String,
    back: String,
}

#[derive(Deserialize)]
pub struct BackQuery {
    #[serde(rename = "return", default)]
    back: Option<String>,
}

pub async fn edit_note(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Query(q): Query<BackQuery>,
) -> Response {
    let data = Data::load(&ctx);
    let Some((s, i)) = find_note(&data.sessions, &id) else {
        return back(
            q.back.as_deref().unwrap_or("/session"),
            Some("That note no longer exists"),
        );
    };
    let note = &data.sessions[s].notes[i];
    let (_, span) = anchor::locate_on_disk(&ctx.root, note);
    let selected: Vec<String> = note.anchor.selected.lines().map(str::to_string).collect();
    let back_to = q.back.unwrap_or_else(|| "/session".into());
    let nav = data.nav(&ctx, "note", back_to.clone(), None);
    render(NotePage {
        nav,
        id,
        location: format!(
            "{}:{}",
            note.file.display(),
            span.unwrap_or(note.span).ui_label()
        ),
        code: highlighted(&ctx, &note.file, &selected),
        text: note.text.clone(),
        session_id: data.sessions[s].id.clone(),
        back: back_to,
    })
}

#[derive(Deserialize)]
pub struct SaveNote {
    text: String,
    session: String,
    #[serde(rename = "return")]
    back: String,
}

/// Edit a note's text; a different session moves it there (to the end).
pub async fn save_note(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Form(form): Form<SaveNote>,
) -> Response {
    let mut data = Data::load(&ctx);
    let Some((from, i)) = find_note(&data.sessions, &id) else {
        return back(&form.back, Some("That note no longer exists"));
    };
    let text = form.text.trim();
    if text.is_empty() {
        return back(
            &format!("/notes/{id}/edit?return={}", enc(&form.back)),
            Some("The note needs some text"),
        );
    }
    let to = data
        .sessions
        .iter()
        .position(|s| s.id == form.session)
        .unwrap_or(from);
    let mut note = data.sessions[from].notes.remove(i);
    note.text = text.to_string();
    if to == from {
        data.sessions[from].notes.insert(i, note);
    } else {
        data.sessions[to].notes.push(note);
        data.save(to);
    }
    data.save(from);
    let msg = if to == from {
        "Note saved".to_string()
    } else {
        format!("Moved to \"{}\"", data.sessions[to].name)
    };
    back(&form.back, Some(&msg))
}

#[derive(Deserialize)]
pub struct BackForm {
    #[serde(rename = "return")]
    back: String,
}

pub async fn delete_note(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Form(form): Form<BackForm>,
) -> Response {
    let mut data = Data::load(&ctx);
    if let Some((s, i)) = find_note(&data.sessions, &id) {
        data.sessions[s].notes.remove(i);
        data.save(s);
    }
    back(&form.back, Some("Note deleted"))
}

#[derive(Deserialize)]
pub struct MoveForm {
    dir: String,
}

/// Session view: one place up or down, which is the prompt order.
pub async fn move_note(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Form(form): Form<MoveForm>,
) -> Response {
    let mut data = Data::load(&ctx);
    if let Some((s, i)) = find_note(&data.sessions, &id) {
        let to = if form.dir == "up" {
            i.checked_sub(1)
        } else {
            Some(i + 1)
        };
        if let Some(to) = to.filter(|&t| t < data.sessions[s].notes.len()) {
            data.sessions[s].notes.swap(i, to);
            data.save(s);
        }
    }
    back("/session", None)
}

#[derive(Deserialize)]
pub struct NewSession {
    name: String,
    #[serde(rename = "return")]
    back: String,
}

pub async fn new_session(State(ctx): State<Ctx>, Form(form): Form<NewSession>) -> Response {
    let name = form.name.trim();
    if name.is_empty() {
        return back(&form.back, Some("A session needs a name"));
    }
    let mut data = Data::load(&ctx);
    let session = Session::new(name);
    data.state.active_session_id = Some(session.id.clone());
    data.sessions.push(session);
    data.save(data.sessions.len() - 1);
    let _ = data.store.save_state(&data.state);
    back(&form.back, Some(&format!("Created session \"{name}\"")))
}

#[derive(Deserialize)]
pub struct Rename {
    name: String,
}

pub async fn rename_session(
    State(ctx): State<Ctx>,
    UrlPath(id): UrlPath<String>,
    Form(form): Form<Rename>,
) -> Response {
    let name = form.name.trim();
    if name.is_empty() {
        return back("/session", Some("A session needs a name"));
    }
    let mut data = Data::load(&ctx);
    let Some(i) = data.sessions.iter().position(|s| s.id == id) else {
        return back("/session", Some("That session is gone"));
    };
    // The file keeps its name, so `tackline export <old-slug>` still works.
    data.sessions[i].name = name.to_string();
    data.save(i);
    back(
        "/session",
        Some(&format!("Renamed the session to \"{name}\"")),
    )
}

#[derive(Deserialize)]
pub struct Activate {
    id: String,
    #[serde(rename = "return")]
    back: String,
}

pub async fn activate_session(State(ctx): State<Ctx>, Form(form): Form<Activate>) -> Response {
    let mut data = Data::load(&ctx);
    if data.sessions.iter().any(|s| s.id == form.id) {
        data.state.active_session_id = Some(form.id);
        let _ = data.store.save_state(&data.state);
    }
    back(&form.back, None)
}
