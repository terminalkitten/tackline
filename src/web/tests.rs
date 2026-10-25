use std::path::Path;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use super::{Ctx, router, safe_path};
use crate::compile::compile;
use crate::highlight::load_theme;
use crate::model::Span;
use crate::store::{FsStore, Store};
use crate::testutil::fixture_repo;

const HOST: &str = "127.0.0.1:7878";

fn ctx(root: &Path) -> Ctx {
    Ctx {
        root: root.to_path_buf(),
        theme: Arc::new(load_theme(Some("base16-ocean.dark")).unwrap()),
        base: None,
        git: true,
        token: "secret".into(),
        port: 7878,
    }
}

/// A request as the browser that opened the link would send it.
async fn send(
    ctx: &Ctx,
    method: &str,
    uri: &str,
    form: Option<&str>,
) -> (StatusCode, String, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::COOKIE, "tackline_token=secret");
    let body = match form {
        Some(f) => {
            req = req.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(f.to_string())
        }
        None => Body::empty(),
    };
    let res = router(ctx.clone())
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let location = res
        .headers()
        .get(header::LOCATION)
        .map(|l| l.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = String::from_utf8(
        to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    (status, location, body)
}

async fn json(
    ctx: &Ctx,
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::COOKIE, "tackline_token=secret");
    let body = match body {
        Some(b) => {
            req = req.header(header::CONTENT_TYPE, "application/json");
            Body::from(b.to_string())
        }
        None => Body::empty(),
    };
    let res = router(ctx.clone())
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

#[test]
fn only_this_browser_on_this_machine() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        let call = |host: &'static str, cookie: Option<&'static str>, uri: &'static str| {
            let ctx = ctx.clone();
            async move {
                let mut req = Request::builder().uri(uri).header(header::HOST, host);
                if let Some(c) = cookie {
                    req = req.header(header::COOKIE, c);
                }
                router(ctx)
                    .oneshot(req.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
            }
        };
        // Another site through DNS rebinding: wrong Host.
        assert_eq!(
            call("evil.example:7878", Some("tackline_token=secret"), "/")
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
        // No token: forbidden, even for the API.
        assert_eq!(call(HOST, None, "/").await.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            call(HOST, None, "/api/sessions").await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(HOST, Some("tackline_token=wrong"), "/").await.status(),
            StatusCode::FORBIDDEN
        );
        // The printed link sets the cookie and drops the token from the URL.
        let res = call("localhost:7878", None, "/?token=secret").await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let cookie = res.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(
            cookie.starts_with("tackline_token=secret;")
                && cookie.contains("HttpOnly")
                && cookie.contains("SameSite=Strict")
        );
        assert_eq!(
            call(HOST, Some("other=1; tackline_token=secret"), "/")
                .await
                .status(),
            StatusCode::OK
        );
    });
}

#[test]
fn paths_stay_in_the_repo() {
    let repo = fixture_repo();
    let root = repo.path();
    assert_eq!(
        safe_path(root, "src/matcher.rs").unwrap(),
        Path::new("src/matcher.rs")
    );
    for bad in [
        "../outside",
        "/etc/passwd",
        "src/../../x",
        ".git/config",
        "",
        "nope.rs",
    ] {
        assert_eq!(safe_path(root, bad), None, "{bad}");
    }
    let ctx = ctx(root);
    run(async {
        let (status, _, _) = send(&ctx, "GET", "/code?path=../../etc/passwd", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _, _) = send(&ctx, "GET", "/code?path=.git/config", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    });
}

#[test]
fn add_a_note_and_see_it() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        let (status, _, page) = send(&ctx, "GET", "/code?path=src/matcher.rs", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("<span style=\"color:#"), "highlighted");
        assert!(page.contains("No sessions yet"));

        let form = "file=src/matcher.rs&view=code&start=3&end=5&text=use+an+iterator&new_session=Perf&return=%2Fcode%3Fpath%3Dsrc%2Fmatcher.rs";
        let (status, location, _) = send(&ctx, "POST", "/notes", Some(form)).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(
            location,
            "/code?path=src/matcher.rs&line=3&msg=Note%20added"
        );

        // Saved like the terminal app saves it.
        let sessions = FsStore::new(repo.path()).load_sessions().unwrap();
        assert_eq!(sessions[0].name, "Perf");
        let note = &sessions[0].notes[0];
        assert_eq!(
            (note.span, note.text.as_str()),
            (Span::lines(3, 5), "use an iterator")
        );
        assert!(note.anchor.selected.starts_with("    for item in items {"));

        let (_, _, page) = send(&ctx, "GET", &location, None).await;
        assert_eq!(
            page.matches("class=\" noted\"").count(),
            3,
            "three tinted lines"
        );
        assert!(page.contains("Note added"));
        assert!(page.contains(&format!("/notes/{}/edit", note.id)));

        // Empty text: refused with a message, nothing saved.
        let form = "file=src/matcher.rs&view=code&start=1&end=1&text=+&return=%2F";
        let (_, location, _) = send(&ctx, "POST", "/notes", Some(form)).await;
        assert!(location.contains("needs%20some%20text"));
        assert_eq!(
            FsStore::new(repo.path()).load_sessions().unwrap()[0]
                .notes
                .len(),
            1
        );
    });
}

#[test]
fn notes_on_the_diff() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        let (status, _, page) = send(&ctx, "GET", "/diff", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("web/app.ts") && page.contains("class=\"add\""));
        // Rows 4-5 are the two added lines (new lines 4-5).
        let form = "file=web/app.ts&view=diff&start=4&end=5&text=read+from+env&new_session=S&return=%2Fdiff";
        send(&ctx, "POST", "/notes", Some(form)).await;
        let note = &FsStore::new(repo.path()).load_sessions().unwrap()[0].notes[0];
        assert_eq!(note.span, Span::lines(4, 5));
        // Row 0 is the hunk header: refused.
        let form = "file=web/app.ts&view=diff&start=0&end=1&text=x&return=%2Fdiff";
        let (_, location, _) = send(&ctx, "POST", "/notes", Some(form)).await;
        assert!(location.contains("Removed%20lines"), "{location}");
    });
}

#[test]
fn session_page_edit_move_reorder_delete_and_prompt() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        // The first note names a new session; the second goes to the active one.
        for (start, text, new) in [(1, "one", "&new_session=A"), (2, "two", "")] {
            let form = format!(
                "file=src/matcher.rs&view=code&start={start}&end={start}&text={text}{new}&return=%2F"
            );
            send(&ctx, "POST", "/notes", Some(&form)).await;
        }
        let store = FsStore::new(repo.path());
        let ids: Vec<String> = store.load_sessions().unwrap()[0]
            .notes
            .iter()
            .map(|n| n.id.clone())
            .collect();

        let (_, _, page) = send(&ctx, "GET", "/session", None).await;
        assert!(
            page.contains("src/matcher.rs:L1")
                && page.contains("two")
                && page.contains("Copy prompt")
        );
        let (_, _, prompt) = send(&ctx, "GET", "/prompt", None).await;
        assert_eq!(
            prompt,
            compile(&store.load_sessions().unwrap()[0], repo.path()).0
        );

        send(
            &ctx,
            "POST",
            &format!("/notes/{}/move", ids[1]),
            Some("dir=up"),
        )
        .await;
        let texts = |s: &FsStore| {
            s.load_sessions().unwrap()[0]
                .notes
                .iter()
                .map(|n| n.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&store), ["two", "one"]);

        let (_, _, edit) = send(
            &ctx,
            "GET",
            &format!("/notes/{}/edit?return=/session", ids[0]),
            None,
        )
        .await;
        assert!(edit.contains(">one</textarea>"));
        let session = store.load_sessions().unwrap()[0].id.clone();
        send(
            &ctx,
            "POST",
            &format!("/notes/{}", ids[0]),
            Some(&format!("text=ONE&session={session}&return=%2Fsession")),
        )
        .await;
        assert_eq!(texts(&store), ["two", "ONE"]);

        // Move to another session.
        send(&ctx, "POST", "/sessions", Some("name=B&return=%2F")).await;
        let b = store.load_sessions().unwrap()[1].id.clone();
        let (_, location, _) = send(
            &ctx,
            "POST",
            &format!("/notes/{}", ids[0]),
            Some(&format!("text=ONE&session={b}&return=%2Fsession")),
        )
        .await;
        assert!(location.contains("Moved"));
        let all = store.load_sessions().unwrap();
        assert_eq!(
            (all[0].notes.len(), all[1].notes[0].text.as_str()),
            (1, "ONE")
        );

        send(
            &ctx,
            "POST",
            &format!("/notes/{}/delete", ids[1]),
            Some("return=%2Fsession"),
        )
        .await;
        assert!(store.load_sessions().unwrap()[0].notes.is_empty());

        // Switching the active session.
        let a = store.load_sessions().unwrap()[0].id.clone();
        send(
            &ctx,
            "POST",
            "/sessions/active",
            Some(&format!("id={a}&return=%2F")),
        )
        .await;
        assert_eq!(store.load_state().unwrap().active_session_id, Some(a));
    });
}

#[test]
fn search_and_tree() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        let (_, _, page) = send(&ctx, "GET", "/search?q=items", None).await;
        assert!(page.contains("src/matcher.rs:3") && page.contains("web/app.ts"));
        let (_, _, page) = send(&ctx, "GET", "/code?path=web/app.ts", None).await;
        assert!(
            page.contains("<details open><summary>web/</summary>"),
            "current folder open"
        );
        let (status, _, js) = send(&ctx, "GET", "/static/htmx.min.js", None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(js.starts_with("var htmx"));
    });
}

#[test]
fn json_api() {
    let repo = fixture_repo();
    let ctx = ctx(repo.path());
    run(async {
        let body = serde_json::json!({ "file": "src/matcher.rs", "start_line": 3, "end_line": 5, "text": "iterate" });
        let (status, created) = json(&ctx, "POST", "/api/notes", Some(body)).await;
        assert_eq!(status, StatusCode::CREATED);
        let (_, list) = json(&ctx, "GET", "/api/sessions", None).await;
        assert_eq!(list[0]["notes"], 1);
        assert_eq!(list[0]["active"], true);

        let (_, session) = json(&ctx, "GET", "/api/sessions/active", None).await;
        assert_eq!(session["notes"][0]["span"]["start_line"], 3);
        assert_eq!(session["notes"][0]["stale"], false);

        let store = FsStore::new(repo.path());
        let (_, prompt) = json(&ctx, "GET", "/api/sessions/active/prompt", None).await;
        assert_eq!(
            prompt["prompt"],
            compile(&store.load_sessions().unwrap()[0], repo.path()).0
        );

        let (status, err) = json(
            &ctx,
            "POST",
            "/api/notes",
            Some(
                serde_json::json!({ "file": "../x", "start_line": 1, "end_line": 1, "text": "x" }),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(err["error"].as_str().unwrap().contains("No such file"));

        let id = created["id"].as_str().unwrap();
        let (status, _) = json(&ctx, "DELETE", &format!("/api/notes/{id}"), None).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert!(store.load_sessions().unwrap()[0].notes.is_empty());
    });
}
