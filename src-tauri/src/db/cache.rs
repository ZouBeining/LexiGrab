// SPDX-License-Identifier: AGPL-3.0-only
//! SQLite lookup cache and sense cache. Contract: docs/BACKEND_STRUCTURE.md §3.
//!
//! One `CacheStore` owns one connection to the shared `cache.db` (the same file
//! `CardStore` uses). Every SQL statement runs behind the store's
//! `Mutex<Connection>`, so the store is `Send + Sync`; blocking callers are
//! expected to run these calls inside `spawn_blocking`.
//!
//! Semantics:
//!
//! - Query keys are normalized with the same `trim + NFKC + lowercase` rule as
//!   [`crate::db::normalize_key`] (which also collapses inner whitespace).
//! - `get` treats an expired row as a miss and deletes it; malformed cached
//!   JSON is also a miss and is deleted, never an error.
//! - Positive and negative entries share one table; `is_empty = 1` marks a
//!   negative (no-result) cache entry. TTLs are chosen by the caller.
//! - `sense_cache` stores selector/LLM resolution JSON keyed by a context key;
//!   failures are never cached.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::{normalize_key, now_unix, open_database, open_in_memory_database, DbError};
use crate::core::entry::Entry;

/// Injectable wall clock so TTL behavior is testable without sleeping.
pub trait Clock: Send + Sync {
    /// Current unix timestamp in seconds.
    fn now(&self) -> i64;
}

/// Clock backed by the system clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> i64 {
        now_unix()
    }
}

/// Result of a lookup-cache read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CachedLookup {
    /// Cached entries; empty for a negative-cache hit.
    pub entries: Vec<Entry>,
    /// Always true for a hit (kept so callers can pass it straight to events).
    pub cached: bool,
}

// `Entry` only derives `PartialEq`, so `Eq` cannot be derived here. The marker
// is still implemented manually to keep the published API `Eq` as specified.
impl Eq for CachedLookup {}

/// Aggregate cache statistics for the settings page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CacheStats {
    /// Total `lookup_cache` rows.
    pub entries: u64,
    /// Rows with `is_empty = 0` (positive entries).
    pub positive_entries: u64,
    /// Rows with `is_empty = 1` (negative entries).
    pub empty_entries: u64,
    /// Total `sense_cache` rows.
    pub sense_entries: u64,
    /// Summed byte size of `lookup_cache.entries_json`.
    pub bytes: u64,
}

/// Lookup and sense cache backed by one SQLite connection.
pub struct CacheStore {
    connection: Mutex<Connection>,
    path: Option<PathBuf>,
    clock: Arc<dyn Clock>,
}

impl CacheStore {
    /// Open (or create) the cache at `path`.
    ///
    /// Parent directories are created on demand. A corrupt database file is
    /// quarantined to `cache.db.corrupt-<unix>` and recreated; lock and
    /// permission errors are reported without touching the file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        let connection = open_database(&path)?;
        Ok(CacheStore {
            connection: Mutex::new(connection),
            path: Some(path),
            clock: Arc::new(SystemClock),
        })
    }

    /// In-memory cache for tests; no file is touched.
    pub fn in_memory() -> Result<Self, DbError> {
        let connection = open_in_memory_database()?;
        Ok(CacheStore {
            connection: Mutex::new(connection),
            path: None,
            clock: Arc::new(SystemClock),
        })
    }

    /// Replace the clock (builder style), typically with a fake clock in tests.
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Path of the backing file, or `None` for in-memory stores.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Read a cached lookup for `source_id` + `query`.
    ///
    /// Returns `None` on a miss, on an expired row (deleted inline) and on
    /// malformed cached JSON (also deleted). A negative-cache hit returns
    /// `Some(CachedLookup { entries: vec![], cached: true })`.
    pub fn get(&self, source_id: &str, query: &str) -> Result<Option<CachedLookup>, DbError> {
        let query_key = normalize_key(query);
        let now = self.clock.now();
        let connection = self.lock()?;

        let row = connection
            .query_row(
                "SELECT entries_json, is_empty, expires_at FROM lookup_cache \
                 WHERE source_id = ?1 AND query_key = ?2",
                rusqlite::params![source_id, query_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(query_error)?;

        let Some((entries_json, is_empty, expires_at)) = row else {
            return Ok(None);
        };

        if expires_at <= now {
            delete_lookup_row(&connection, source_id, &query_key)?;
            return Ok(None);
        }

        if is_empty != 0 {
            return Ok(Some(CachedLookup {
                entries: Vec::new(),
                cached: true,
            }));
        }

        match serde_json::from_str::<Vec<Entry>>(&entries_json) {
            Ok(entries) => Ok(Some(CachedLookup {
                entries,
                cached: true,
            })),
            Err(error) => {
                // Cached JSON is an implementation detail; a broken row is
                // dropped instead of surfacing an error to the query path.
                tracing::warn!(
                    source_id,
                    error = %error,
                    "dropping malformed lookup cache row"
                );
                delete_lookup_row(&connection, source_id, &query_key)?;
                Ok(None)
            }
        }
    }

    /// Store a non-empty lookup result, replacing any previous row.
    pub fn put_entries(
        &self,
        source_id: &str,
        query: &str,
        entries: &[Entry],
        ttl_secs: i64,
    ) -> Result<(), DbError> {
        let entries_json = serde_json::to_string(entries)
            .map_err(|error| DbError::Query(format!("serialize cached entries: {error}")))?;
        self.put_row(source_id, query, &entries_json, false, ttl_secs)
    }

    /// Store a negative (no-result) lookup, replacing any previous row.
    pub fn put_empty(&self, source_id: &str, query: &str, ttl_secs: i64) -> Result<(), DbError> {
        self.put_row(source_id, query, "[]", true, ttl_secs)
    }

    /// Drop every cached lookup for one source (source modified or removed).
    pub fn invalidate_source(&self, source_id: &str) -> Result<u64, DbError> {
        let connection = self.lock()?;
        let removed = connection
            .execute("DELETE FROM lookup_cache WHERE source_id = ?1", [source_id])
            .map_err(query_error)?;
        count_to_u64(removed)
    }

    /// Drop every cached lookup; returns the number of removed rows.
    pub fn clear_lookup(&self) -> Result<u64, DbError> {
        let connection = self.lock()?;
        let removed = connection
            .execute("DELETE FROM lookup_cache", [])
            .map_err(query_error)?;
        count_to_u64(removed)
    }

    /// Drop every cached sense resolution; returns the number of removed rows.
    pub fn clear_sense(&self) -> Result<u64, DbError> {
        let connection = self.lock()?;
        let removed = connection
            .execute("DELETE FROM sense_cache", [])
            .map_err(query_error)?;
        count_to_u64(removed)
    }

    /// Alias for [`Self::clear_sense`].
    pub fn sense_clear(&self) -> Result<u64, DbError> {
        self.clear_sense()
    }

    /// Current cache statistics.
    pub fn stats(&self) -> Result<CacheStats, DbError> {
        let connection = self.lock()?;
        let entries = count_query(&connection, "SELECT COUNT(*) FROM lookup_cache")?;
        let positive_entries = count_query(
            &connection,
            "SELECT COUNT(*) FROM lookup_cache WHERE is_empty = 0",
        )?;
        let empty_entries = count_query(
            &connection,
            "SELECT COUNT(*) FROM lookup_cache WHERE is_empty = 1",
        )?;
        let sense_entries = count_query(&connection, "SELECT COUNT(*) FROM sense_cache")?;
        let bytes: i64 = connection
            .query_row(
                // LENGTH of a BLOB counts bytes; of TEXT it counts characters.
                "SELECT COALESCE(SUM(LENGTH(CAST(entries_json AS BLOB))), 0) \
                 FROM lookup_cache",
                [],
                |row| row.get(0),
            )
            .map_err(query_error)?;
        Ok(CacheStats {
            entries,
            positive_entries,
            empty_entries,
            sense_entries,
            bytes: count_to_u64_from_i64(bytes)?,
        })
    }

    /// Remove expired lookup and sense rows; returns the total removed.
    pub fn cleanup_expired(&self) -> Result<u64, DbError> {
        let now = self.clock.now();
        let connection = self.lock()?;
        let lookup = connection
            .execute("DELETE FROM lookup_cache WHERE expires_at <= ?1", [now])
            .map_err(query_error)?;
        let sense = connection
            .execute("DELETE FROM sense_cache WHERE expires_at <= ?1", [now])
            .map_err(query_error)?;
        count_to_u64(lookup.saturating_add(sense))
    }

    /// Read a cached sense resolution by context key.
    pub fn sense_get(&self, context_key: &str) -> Result<Option<String>, DbError> {
        let now = self.clock.now();
        let connection = self.lock()?;
        let row = connection
            .query_row(
                "SELECT resolution_json, expires_at FROM sense_cache \
                 WHERE context_key = ?1",
                [context_key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(query_error)?;

        match row {
            Some((resolution_json, expires_at)) if expires_at > now => Ok(Some(resolution_json)),
            Some(_) => {
                connection
                    .execute(
                        "DELETE FROM sense_cache WHERE context_key = ?1",
                        [context_key],
                    )
                    .map_err(query_error)?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    /// Store a sense resolution JSON, replacing any previous row.
    pub fn sense_put(
        &self,
        context_key: &str,
        resolution_json: &str,
        ttl_secs: i64,
    ) -> Result<(), DbError> {
        let now = self.clock.now();
        let expires_at = now.saturating_add(ttl_secs);
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO sense_cache \
                 (context_key, resolution_json, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(context_key) DO UPDATE SET \
                   resolution_json = excluded.resolution_json, \
                   created_at = excluded.created_at, \
                   expires_at = excluded.expires_at",
                rusqlite::params![context_key, resolution_json, now, expires_at],
            )
            .map_err(query_error)?;
        Ok(())
    }

    /// Upsert one `lookup_cache` row.
    fn put_row(
        &self,
        source_id: &str,
        query: &str,
        entries_json: &str,
        is_empty: bool,
        ttl_secs: i64,
    ) -> Result<(), DbError> {
        let query_key = normalize_key(query);
        let now = self.clock.now();
        let expires_at = now.saturating_add(ttl_secs);
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO lookup_cache \
                 (source_id, query_key, entries_json, is_empty, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
                 ON CONFLICT(source_id, query_key) DO UPDATE SET \
                   entries_json = excluded.entries_json, \
                   is_empty = excluded.is_empty, \
                   created_at = excluded.created_at, \
                   expires_at = excluded.expires_at",
                rusqlite::params![
                    source_id,
                    query_key,
                    entries_json,
                    is_empty,
                    now,
                    expires_at
                ],
            )
            .map_err(query_error)?;
        Ok(())
    }

    /// Serialize access to the connection, reporting a poisoned lock.
    fn lock(&self) -> Result<MutexGuard<'_, Connection>, DbError> {
        self.connection.lock().map_err(|_| DbError::Lock)
    }
}

/// Delete one looked-up row after it expired or failed to decode.
fn delete_lookup_row(
    connection: &Connection,
    source_id: &str,
    query_key: &str,
) -> Result<(), DbError> {
    connection
        .execute(
            "DELETE FROM lookup_cache WHERE source_id = ?1 AND query_key = ?2",
            rusqlite::params![source_id, query_key],
        )
        .map_err(query_error)?;
    Ok(())
}

/// Run a `COUNT(*)` query and convert the result.
fn count_query(connection: &Connection, sql: &str) -> Result<u64, DbError> {
    let count: i64 = connection
        .query_row(sql, [], |row| row.get(0))
        .map_err(query_error)?;
    count_to_u64_from_i64(count)
}

/// Convert a row count to `u64`.
fn count_to_u64(count: usize) -> Result<u64, DbError> {
    u64::try_from(count).map_err(|_| DbError::Query(format!("row count out of range: {count}")))
}

/// Convert a SQLite integer to `u64`.
fn count_to_u64_from_i64(count: i64) -> Result<u64, DbError> {
    u64::try_from(count).map_err(|_| DbError::Query(format!("row count out of range: {count}")))
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
    fn cache_store_is_send_and_sync() {
        assert_send_sync::<CacheStore>();
    }
}
