//! SQLite store for saved-card history, lookup cache and sense cache.
//! Contract: docs/BACKEND_STRUCTURE.md §3.
//!
//! `saved_cards` and the "same word + sentence + definition" context check live
//! here; the lookup/sense caches live in [`cache`]. All SQL is serialized
//! behind one `Mutex<Connection>` per store, so both `CardStore` and
//! `CacheStore` are `Send + Sync` and safe to share through Tauri state.
//! Blocking callers are expected to run store calls inside `spawn_blocking`
//! (the Lead's command layer does).
//!
//! Both stores open their own connection to the same file
//! (`%LOCALAPPDATA%\LexiGrab\cache.db`). Each connection applies WAL
//! journaling, a 2s busy timeout and `user_version = 2`, and runs the
//! idempotent `CREATE ... IF NOT EXISTS` schema, so the open order is
//! irrelevant. A corrupt database file is renamed to
//! `cache.db.corrupt-<unix>` and recreated instead of failing the app.

pub mod cache;
pub mod fingerprint;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::core::error::IpcError;
pub use fingerprint::{definition_key, definition_text, normalize_key, sentence_key};

/// Schema version written to `PRAGMA user_version`.
pub const SCHEMA_VERSION: u32 = 2;

/// SQLite busy timeout in milliseconds.
pub const BUSY_TIMEOUT_MS: u64 = 2000;

/// Full database schema, verbatim from docs/BACKEND_STRUCTURE.md §3.
///
/// Every statement is idempotent so the same batch can run from both the
/// `CardStore` and `CacheStore` connections.
const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS lookup_cache (
  source_id    TEXT NOT NULL,
  query_key    TEXT NOT NULL,
  entries_json TEXT NOT NULL,
  is_empty     INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL,
  expires_at   INTEGER NOT NULL,
  PRIMARY KEY (source_id, query_key)
);
CREATE INDEX IF NOT EXISTS idx_lookup_cache_expires
  ON lookup_cache(expires_at);

CREATE TABLE IF NOT EXISTS saved_cards (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  word            TEXT NOT NULL,
  sentence_key    TEXT NOT NULL,
  definition_key  TEXT NOT NULL,
  source          TEXT NOT NULL,
  note_id         INTEGER,
  created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_saved_cards_context
  ON saved_cards(word, sentence_key, definition_key);

CREATE TABLE IF NOT EXISTS sense_cache (
  context_key     TEXT PRIMARY KEY,
  resolution_json TEXT NOT NULL,
  created_at      INTEGER NOT NULL,
  expires_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sense_cache_expires
  ON sense_cache(expires_at);
"#;

/// Errors produced by the saved-card and cache stores.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DbError {
    #[error("failed to open card store: {0}")]
    Open(String),
    #[error("card store query failed: {0}")]
    Query(String),
    #[error("failed to recover card store: {0}")]
    Recover(String),
    #[error("card store lock poisoned")]
    Lock,
}

impl DbError {
    /// Fixed IPC error code for database failures.
    pub fn code(&self) -> &'static str {
        "INTERNAL"
    }
}

impl From<DbError> for IpcError {
    fn from(error: DbError) -> Self {
        IpcError::new(error.code(), error.to_string()).with_detail(error.to_string())
    }
}

/// Result of a "same word + sentence + definition" context check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CardContext {
    pub seen: bool,
    pub note_id: Option<i64>,
    pub created_at: Option<i64>,
}

/// Saved-card store backed by one SQLite connection.
pub struct CardStore {
    connection: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl CardStore {
    /// Application database path: `%LOCALAPPDATA%\LexiGrab\cache.db`, or
    /// `$LEXIGRAB_HOME/cache.db` when the test/portable override is set.
    pub fn default_path() -> PathBuf {
        if let Some(root) = std::env::var_os("LEXIGRAB_HOME") {
            return PathBuf::from(root).join("cache.db");
        }
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LexiGrab")
            .join("cache.db")
    }

    /// Open (or create) the store at `path`.
    ///
    /// Parent directories are created on demand. If the existing file is not a
    /// usable SQLite database it is renamed aside and a fresh one is created.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        let connection = open_database(&path)?;
        Ok(CardStore {
            connection: Mutex::new(connection),
            path: Some(path),
        })
    }

    /// In-memory store for tests; no file is touched.
    pub fn in_memory() -> Result<Self, DbError> {
        let connection = open_in_memory_database()?;
        Ok(CardStore {
            connection: Mutex::new(connection),
            path: None,
        })
    }

    /// Path of the backing file, or `None` for in-memory stores.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Look up whether the exact word + sentence + definition context was saved.
    ///
    /// `definition_text` is plain text (callers convert HTML with
    /// [`fingerprint::definition_text`] first); the fingerprint does not parse
    /// it as HTML again.
    pub fn check_context(
        &self,
        word: &str,
        sentence: Option<&str>,
        definition_text: &str,
    ) -> Result<CardContext, DbError> {
        let word_key = normalize_key(word);
        let sentence_key = sentence_key_for(sentence);
        let definition_key = fingerprint::definition_key_from_text(definition_text);

        let connection = self.lock()?;
        let row = connection
            .query_row(
                "SELECT note_id, created_at FROM saved_cards \
                 WHERE word = ?1 AND sentence_key = ?2 AND definition_key = ?3 \
                 ORDER BY created_at DESC, id DESC LIMIT 1",
                rusqlite::params![word_key, sentence_key, definition_key],
                |row| {
                    let note_id: Option<i64> = row.get(0)?;
                    let created_at: i64 = row.get(1)?;
                    Ok((note_id, created_at))
                },
            )
            .optional()
            .map_err(query_error)?;

        Ok(match row {
            Some((note_id, created_at)) => CardContext {
                seen: true,
                note_id,
                created_at: Some(created_at),
            },
            None => CardContext {
                seen: false,
                note_id: None,
                created_at: None,
            },
        })
    }

    /// Record a successfully exported card so later checks can warn about it.
    ///
    /// `definition_text` is plain text, same contract as [`Self::check_context`].
    pub fn record_card(
        &self,
        word: &str,
        sentence: Option<&str>,
        definition_text: &str,
        source: &str,
        note_id: i64,
    ) -> Result<(), DbError> {
        let word_key = normalize_key(word);
        let sentence_key = sentence_key_for(sentence);
        let definition_key = fingerprint::definition_key_from_text(definition_text);

        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO saved_cards \
                 (word, sentence_key, definition_key, source, note_id, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    word_key,
                    sentence_key,
                    definition_key,
                    source,
                    note_id,
                    now_unix()
                ],
            )
            .map_err(query_error)?;
        Ok(())
    }

    /// Number of saved cards (used by tests and status reporting).
    pub fn count(&self) -> Result<u64, DbError> {
        let connection = self.lock()?;
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM saved_cards", [], |row| row.get(0))
            .map_err(query_error)?;
        u64::try_from(count)
            .map_err(|_| DbError::Query(format!("saved_cards count out of range: {count}")))
    }

    /// Serialize access to the connection, reporting a poisoned lock.
    fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.connection.lock().map_err(|_| DbError::Lock)
    }
}

/// Open (or create) a file-backed database, quarantining a corrupt file.
///
/// Parent directories are created on demand. If the existing file is not a
/// usable SQLite database it is renamed aside and a fresh one is created; a
/// locked or permission-denied but structurally valid database is returned as
/// an error untouched.
pub(crate) fn open_database(path: &Path) -> Result<Connection, DbError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                DbError::Open(format!(
                    "create directory {} failed: {error}",
                    parent.display()
                ))
            })?;
        }
    }

    match open_at(path) {
        Ok(connection) => Ok(connection),
        Err(error) => {
            // Only recognized corruption triggers quarantine; a locked or
            // permission-denied valid database must never be renamed away.
            if !should_quarantine(path, &error) {
                return Err(error);
            }
            tracing::warn!(
                path = %path.display(),
                error = %error,
                "cache database is unusable; moving it aside"
            );
            let backup = quarantine(path)?;
            let connection = open_at(path).map_err(|retry| {
                DbError::Recover(format!(
                    "{retry} (original error: {error}; quarantined to {})",
                    backup.display()
                ))
            })?;
            tracing::warn!(
                path = %path.display(),
                backup = %backup.display(),
                "recreated cache database after corruption"
            );
            Ok(connection)
        }
    }
}

/// Open (or create) an in-memory database and initialize schema/pragmas.
pub(crate) fn open_in_memory_database() -> Result<Connection, DbError> {
    let connection = Connection::open_in_memory()
        .map_err(|error| DbError::Open(format!("open in-memory database: {error}")))?;
    init_connection(connection)
}

/// Open a file-backed connection and initialize schema/pragmas.
fn open_at(path: &Path) -> Result<Connection, DbError> {
    let connection = Connection::open(path)
        .map_err(|error| DbError::Open(format!("open {}: {error}", path.display())))?;
    init_connection(connection)
}

/// Apply pragmas and create the schema for a fresh connection.
fn init_connection(connection: Connection) -> Result<Connection, DbError> {
    connection
        .busy_timeout(Duration::from_millis(BUSY_TIMEOUT_MS))
        .map_err(query_error)?;
    // journal_mode returns a row; execute_batch discards it on purpose.
    connection
        .execute_batch("PRAGMA journal_mode = WAL;")
        .map_err(query_error)?;
    connection
        .pragma_update(None, "user_version", i64::from(SCHEMA_VERSION))
        .map_err(query_error)?;
    connection.execute_batch(SCHEMA_SQL).map_err(query_error)?;
    Ok(connection)
}

/// Whether an open/init failure should be treated as database corruption.
///
/// A locked or permission-denied but structurally valid database must not be
/// renamed away, so quarantine requires one of: an empty file, a missing or
/// foreign SQLite header, or an explicit malformed/not-a-database error.
fn should_quarantine(path: &Path, error: &DbError) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if metadata.len() == 0 {
        return true;
    }
    let mut header = [0_u8; 16];
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if std::io::Read::read_exact(&mut file, &mut header).is_err() {
        return true;
    }
    if &header != b"SQLite format 3\0" {
        return true;
    }
    let message = error.to_string().to_lowercase();
    message.contains("malformed")
        || message.contains("not a database")
        || message.contains("corrupt")
}

/// Move an unusable database (and its WAL sidecars) out of the way.
fn quarantine(path: &Path) -> Result<PathBuf, DbError> {
    let backup = corrupt_backup_path(path);
    std::fs::rename(path, &backup).map_err(|error| {
        DbError::Recover(format!(
            "move {} to {} failed: {error}",
            path.display(),
            backup.display()
        ))
    })?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = sidecar_path(path, suffix);
        match std::fs::remove_file(&sidecar) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                path = %sidecar.display(),
                error = %error,
                "failed to remove stale sqlite sidecar"
            ),
        }
    }
    Ok(backup)
}

/// `cache.db` -> `cache.db.corrupt-<unix>`, de-duplicated if needed.
fn corrupt_backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "cache.db".to_string());
    let stamp = now_unix();
    let mut candidate = path.with_file_name(format!("{file_name}.corrupt-{stamp}"));
    let mut counter = 1_u32;
    while candidate.exists() {
        candidate = path.with_file_name(format!("{file_name}.corrupt-{stamp}-{counter}"));
        counter += 1;
    }
    candidate
}

/// Append a SQLite sidecar suffix (`-wal`, `-shm`) to a database path.
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut raw = path.as_os_str().to_os_string();
    raw.push(suffix);
    PathBuf::from(raw)
}

/// Fingerprint for an optional sentence; absent and blank share one key.
fn sentence_key_for(sentence: Option<&str>) -> String {
    sentence_key(sentence.unwrap_or_default())
}

/// Current unix timestamp in seconds; 0 on clocks before the epoch.
pub(crate) fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// Map rusqlite errors to the store's query error.
fn query_error(error: rusqlite::Error) -> DbError {
    DbError::Query(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn card_store_is_send_and_sync() {
        assert_send_sync::<CardStore>();
    }

    #[test]
    fn in_memory_store_starts_empty() {
        let store = CardStore::in_memory().expect("open store");
        assert_eq!(store.count().expect("count"), 0);
        assert!(store.path().is_none());
    }

    #[test]
    fn recorded_context_is_seen_with_note_id() {
        let store = CardStore::in_memory().expect("open store");
        // `definition_text` arguments are plain text; callers strip HTML first.
        store
            .record_card("Hello", Some("Hello world."), "hi", "Mini", 42)
            .expect("record card");
        let context = store
            .check_context("hello", Some("  hello   world. "), "  Hi ")
            .expect("check context");
        assert!(context.seen);
        assert_eq!(context.note_id, Some(42));
        assert!(context.created_at.is_some());
        assert_eq!(store.count().expect("count"), 1);
    }

    #[test]
    fn different_sentence_or_definition_is_not_seen() {
        let store = CardStore::in_memory().expect("open store");
        store
            .record_card("hello", Some("first sentence"), "hi", "Mini", 1)
            .expect("record card");
        assert!(
            !store
                .check_context("hello", Some("second sentence"), "hi")
                .expect("check context")
                .seen
        );
        assert!(
            !store
                .check_context("hello", Some("first sentence"), "bye")
                .expect("check context")
                .seen
        );
    }
}
