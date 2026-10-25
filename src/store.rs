//! Sessions and app state on disk, under `.tackline/`.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{Session, State};

const VERSION: u32 = 1;
pub const DATA_DIR: &str = ".tackline";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{path}: unsupported version {version} (expected {VERSION})")]
    Version { path: PathBuf, version: u32 },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// All disk I/O for sessions and state goes through here, so `update()` can
/// be tested without real files.
pub trait Store {
    /// All sessions, oldest first.
    fn load_sessions(&self) -> Result<Vec<Session>>;
    fn save_session(&self, session: &Session) -> Result<()>;
    fn delete_session(&self, id: &str) -> Result<()>;
    /// Default state when the file doesn't exist yet.
    fn load_state(&self) -> Result<State>;
    fn save_state(&self, state: &State) -> Result<()>;
}

/// Every JSON file carries `version` next to its data.
#[derive(Serialize, Deserialize)]
struct Versioned<T> {
    version: u32,
    #[serde(flatten)]
    data: T,
}

/// `.tackline/sessions/<slug>.json` and `.tackline/state.json` under `root`.
pub struct FsStore {
    dir: PathBuf,
}

impl FsStore {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            dir: repo_root.join(DATA_DIR),
        }
    }

    fn sessions_dir(&self) -> PathBuf {
        self.dir.join("sessions")
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    /// Session files with their sessions.
    // ponytail: parses every file on each save to find the id's file; cache
    // id -> path if sessions ever number in the thousands.
    fn session_files(&self) -> Result<Vec<(PathBuf, Session)>> {
        let dir = self.sessions_dir();
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(StoreError::Io { path: dir, source }),
        };
        let mut files = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|source| StoreError::Io {
                    path: dir.clone(),
                    source,
                })?
                .path();
            if path.extension().is_some_and(|e| e == "json") {
                let session = read_json(&path)?;
                files.push((path, session));
            }
        }
        Ok(files)
    }
}

impl Store for FsStore {
    fn load_sessions(&self) -> Result<Vec<Session>> {
        let mut sessions: Vec<Session> =
            self.session_files()?.into_iter().map(|(_, s)| s).collect();
        sessions.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        Ok(sessions)
    }

    fn save_session(&self, session: &Session) -> Result<()> {
        let files = self.session_files()?;
        let path = match files.iter().find(|(_, s)| s.id == session.id) {
            Some((path, _)) => path.clone(),
            None => {
                let base = slug(&session.name);
                let dir = self.sessions_dir();
                (1..)
                    .map(|n| match n {
                        1 => dir.join(format!("{base}.json")),
                        n => dir.join(format!("{base}-{n}.json")),
                    })
                    .find(|p| !files.iter().any(|(f, _)| f == p))
                    .expect("unbounded range")
            }
        };
        write_json(&path, session)
    }

    fn delete_session(&self, id: &str) -> Result<()> {
        for (path, _) in self
            .session_files()?
            .into_iter()
            .filter(|(_, s)| s.id == id)
        {
            fs::remove_file(&path).map_err(|source| StoreError::Io { path, source })?;
        }
        Ok(())
    }

    fn load_state(&self) -> Result<State> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(State::default());
        }
        read_json(&path)
    }

    fn save_state(&self, state: &State) -> Result<()> {
        write_json(&self.state_path(), state)
    }
}

/// In-memory store for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemStore {
    sessions: std::cell::RefCell<Vec<Session>>,
    state: std::cell::RefCell<State>,
}

#[cfg(test)]
impl Store for MemStore {
    fn load_sessions(&self) -> Result<Vec<Session>> {
        Ok(self.sessions.borrow().clone())
    }

    fn save_session(&self, session: &Session) -> Result<()> {
        let mut sessions = self.sessions.borrow_mut();
        match sessions.iter_mut().find(|s| s.id == session.id) {
            Some(s) => *s = session.clone(),
            None => sessions.push(session.clone()),
        }
        Ok(())
    }

    fn delete_session(&self, id: &str) -> Result<()> {
        self.sessions.borrow_mut().retain(|s| s.id != id);
        Ok(())
    }

    fn load_state(&self) -> Result<State> {
        Ok(self.state.borrow().clone())
    }

    fn save_state(&self, state: &State) -> Result<()> {
        *self.state.borrow_mut() = state.clone();
        Ok(())
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|source| StoreError::Io {
        path: path.into(),
        source,
    })?;
    let v: Versioned<T> = serde_json::from_slice(&bytes).map_err(|source| StoreError::Json {
        path: path.into(),
        source,
    })?;
    if v.version != VERSION {
        return Err(StoreError::Version {
            path: path.into(),
            version: v.version,
        });
    }
    Ok(v.data)
}

/// Atomic: write a temp file in the same directory, then rename over `path`.
fn write_json<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let io_err = |source| StoreError::Io {
        path: path.into(),
        source,
    };
    let dir = path.parent().expect("store paths have a parent");
    fs::create_dir_all(dir).map_err(io_err)?;
    let json = serde_json::to_vec_pretty(&Versioned {
        version: VERSION,
        data,
    })
    .map_err(|source| StoreError::Json {
        path: path.into(),
        source,
    })?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(io_err)?;
    tmp.write_all(&json).map_err(io_err)?;
    tmp.as_file().sync_all().map_err(io_err)?;
    tmp.persist(path).map_err(|e| io_err(e.error))?;
    Ok(())
}

/// `"Perf: refactor #2"` → `"perf-refactor-2"`; empty → `"session"`.
pub fn slug(name: &str) -> String {
    let slug = slug::slugify(name);
    if slug.is_empty() {
        "session".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Note, Span, View};

    #[test]
    fn slugs() {
        assert_eq!(slug("Perf: refactor #2"), "perf-refactor-2");
        assert_eq!(slug("  Überarbeitung  "), "uberarbeitung");
        assert_eq!(slug("!!!"), "session");
    }

    #[test]
    fn session_roundtrip_and_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FsStore::new(tmp.path());
        assert!(store.load_sessions().unwrap().is_empty());

        let mut session = Session::new("Perf refactor");
        store.save_session(&session).unwrap();
        let file = tmp.path().join(".tackline/sessions/perf-refactor.json");
        let raw: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(raw["version"], 1);
        assert_eq!(raw["name"], "Perf refactor");

        let anchor = crate::anchor::capture(Span::lines(1, 1), "x").unwrap();
        session.notes.push(Note::new(
            "src/lib.rs".into(),
            Span::lines(1, 1),
            anchor,
            "do it",
        ));
        store.save_session(&session).unwrap();
        assert_eq!(store.load_sessions().unwrap(), vec![session.clone()]);

        store.delete_session(&session.id).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn slug_collisions_get_suffix_and_stay_stable() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FsStore::new(tmp.path());
        let [a, b, c] = ["Fix", "fix!", "FIX"].map(Session::new);
        for s in [&a, &b, &c, &b] {
            store.save_session(s).unwrap();
        }
        let mut names: Vec<_> = fs::read_dir(store.sessions_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["fix-2.json", "fix-3.json", "fix.json"]);
        let mut ids: Vec<_> = store
            .load_sessions()
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        let mut want = [a.id, b.id, c.id];
        ids.sort();
        want.sort();
        assert_eq!(ids, want);
    }

    #[test]
    fn state_defaults_then_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FsStore::new(tmp.path());
        assert_eq!(store.load_state().unwrap(), State::default());
        let state = State {
            active_session_id: Some("abc".into()),
            last_view: Some(View::Diff),
            last_file: Some("src/main.rs".into()),
        };
        store.save_state(&state).unwrap();
        assert_eq!(store.load_state().unwrap(), state);
        let raw = fs::read_to_string(tmp.path().join(".tackline/state.json")).unwrap();
        assert!(raw.contains("\"version\": 1") && raw.contains("\"last_view\": \"diff\""));
    }

    #[test]
    fn rejects_unknown_version() {
        let tmp = tempfile::tempdir().unwrap();
        let store = FsStore::new(tmp.path());
        fs::create_dir_all(tmp.path().join(".tackline")).unwrap();
        fs::write(store.state_path(), r#"{"version": 2}"#).unwrap();
        assert!(matches!(
            store.load_state(),
            Err(StoreError::Version { version: 2, .. })
        ));
    }
}
