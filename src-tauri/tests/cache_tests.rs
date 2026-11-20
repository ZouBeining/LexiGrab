// SPDX-License-Identifier: AGPL-3.0-only
//! Integration tests for the SQLite lookup/sense cache. Contract:
//! docs/BACKEND_STRUCTURE.md §3 and PLAN_M4 T4.4.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use lexigrab_lib::core::entry::{Entry, ResourceRef};
use lexigrab_lib::db::cache::{CacheStore, Clock};
use lexigrab_lib::db::CardStore;

/// Isolated temp dir under `src-tauri/target/test-tmp` (never `%TEMP%`).
fn temp_dir(tag: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-tmp")
        .join("cache-tests")
        .join(tag);
    if root.exists() {
        let _ = std::fs::remove_dir_all(&root);
    }
    std::fs::create_dir_all(&root).expect("create temp dir");
    root
}

/// Clock controlled by the test, starting at a fixed unix second.
struct FakeClock {
    now: AtomicI64,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: AtomicI64::new(1_700_000_000),
        }
    }

    fn advance(&self, secs: i64) {
        self.now.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
}

/// In-memory store plus the fake clock that drives its TTLs.
fn store_with_clock() -> (CacheStore, Arc<FakeClock>) {
    let clock = Arc::new(FakeClock::new());
    let store = CacheStore::in_memory()
        .expect("open in-memory cache")
        .with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
    (store, clock)
}

fn sample_entry(headword: &str) -> Entry {
    Entry {
        headword: headword.to_string(),
        definition_html: "<b>hi</b>".to_string(),
        phonetic: Some("/hɪ/".to_string()),
        resources: vec![ResourceRef::Remote {
            url: "https://example.com/a.png".to_string(),
        }],
        source_name: "Mini".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Positive and negative hits
// ---------------------------------------------------------------------------

#[test]
fn positive_lookup_hits_until_ttl_expires() {
    let (store, clock) = store_with_clock();
    assert!(store.path().is_none(), "in-memory store has no path");

    let entries = vec![sample_entry("hello")];
    // Query keys are trim + NFKC + lowercase; the stored key folds too.
    store
        .put_entries("remote-a", "  Ｈｅｌｌｏ ", &entries, 60)
        .expect("put entries");

    let hit = store
        .get("remote-a", "hello")
        .expect("get cached")
        .expect("cache hit");
    assert!(hit.cached);
    assert_eq!(hit.entries, entries);

    clock.advance(59);
    assert!(store.get("remote-a", "hello").expect("get").is_some());

    clock.advance(2);
    assert_eq!(store.get("remote-a", "hello").expect("get"), None);
    // An expired row is deleted inline, so it no longer shows up in stats.
    assert_eq!(store.stats().expect("stats").entries, 0);
}

#[test]
fn negative_cache_hit_returns_empty_entries() {
    let (store, clock) = store_with_clock();
    store
        .put_empty("remote-a", "missing", 300)
        .expect("put empty");

    let hit = store
        .get("remote-a", "missing")
        .expect("get")
        .expect("negative hit");
    assert!(hit.cached);
    assert_eq!(hit.entries, Vec::new());

    let stats = store.stats().expect("stats");
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.positive_entries, 0);
    assert_eq!(stats.empty_entries, 1);
    assert_eq!(stats.bytes, 2, "negative row stores the JSON `[]`");

    clock.advance(300);
    assert_eq!(store.get("remote-a", "missing").expect("get"), None);
}

#[test]
fn invalidate_source_only_removes_that_source() {
    let (store, _clock) = store_with_clock();
    store
        .put_entries("remote-a", "hello", &[sample_entry("hello")], 60)
        .expect("put a");
    store
        .put_entries("remote-a", "bye", &[sample_entry("bye")], 60)
        .expect("put a second");
    store
        .put_entries("remote-b", "hello", &[sample_entry("hello")], 60)
        .expect("put b");

    assert_eq!(store.invalidate_source("remote-a").expect("invalidate"), 2);
    assert_eq!(store.get("remote-a", "hello").expect("get"), None);
    assert!(store.get("remote-b", "hello").expect("get").is_some());
    assert_eq!(store.invalidate_source("remote-a").expect("again"), 0);
}

// ---------------------------------------------------------------------------
// Sense cache
// ---------------------------------------------------------------------------

#[test]
fn sense_cache_hits_until_ttl_expires() {
    let (store, clock) = store_with_clock();
    let resolution = r#"{"status":"selected","selected_sense_id":"e0-s1"}"#;
    store
        .sense_put("ctx-1", resolution, 3600)
        .expect("sense put");
    assert!(store
        .get("remote-a", "hello")
        .expect("lookup get")
        .is_none());
    assert_eq!(
        store.sense_get("ctx-1").expect("sense get").as_deref(),
        Some(resolution)
    );
    assert_eq!(store.stats().expect("stats").sense_entries, 1);

    clock.advance(3600);
    assert_eq!(store.sense_get("ctx-1").expect("sense get"), None);
    assert_eq!(
        store.stats().expect("stats").sense_entries,
        0,
        "expired sense row is deleted on read"
    );
}

#[test]
fn sense_put_replaces_and_clear_counts_rows() {
    let (store, _clock) = store_with_clock();
    store.sense_put("ctx-1", "old", 60).expect("sense put");
    store.sense_put("ctx-1", "new", 60).expect("sense replace");
    store.sense_put("ctx-2", "other", 60).expect("sense put");
    assert_eq!(
        store.sense_get("ctx-1").expect("sense get").as_deref(),
        Some("new")
    );
    assert_eq!(store.sense_clear().expect("sense clear"), 2);
    assert_eq!(store.sense_clear().expect("sense clear again"), 0);
    assert_eq!(store.stats().expect("stats").sense_entries, 0);
    assert_eq!(store.clear_sense().expect("clear sense"), 0);
}

// ---------------------------------------------------------------------------
// Statistics and clearing
// ---------------------------------------------------------------------------

#[test]
fn stats_count_kinds_and_json_bytes() {
    let (store, _clock) = store_with_clock();
    let entries = vec![sample_entry("hello"), sample_entry("world")];
    // Multi-byte text: byte size must use BLOB length, not character count.
    let mut non_ascii = sample_entry("你好");
    non_ascii.definition_html = "<b>你好，世界</b>".to_string();
    store
        .put_entries("remote-a", "hello", &entries, 60)
        .expect("put entries");
    store
        .put_entries("remote-a", "你好", &[non_ascii], 60)
        .expect("put non-ascii");
    store
        .put_empty("remote-a", "missing", 60)
        .expect("put empty");

    let expected_bytes = serde_json::to_string(&entries)
        .expect("serialize entries")
        .len() as u64
        + serde_json::to_string(
            &store
                .get("remote-a", "你好")
                .expect("get")
                .expect("hit")
                .entries,
        )
        .expect("serialize cached entries")
        .len() as u64
        + 2;

    let stats = store.stats().expect("stats");
    assert_eq!(stats.entries, 3);
    assert_eq!(stats.positive_entries, 2);
    assert_eq!(stats.empty_entries, 1);
    assert_eq!(stats.sense_entries, 0);
    assert_eq!(stats.bytes, expected_bytes);
    assert!(
        stats.bytes
            > serde_json::to_string(&entries)
                .expect("serialize entries")
                .chars()
                .count() as u64,
        "byte accounting must include multi-byte payloads"
    );

    assert_eq!(store.clear_lookup().expect("clear lookup"), 3);
    assert_eq!(store.clear_lookup().expect("clear lookup again"), 0);
    let stats = store.stats().expect("stats");
    assert_eq!(stats.entries, 0);
    assert_eq!(stats.bytes, 0);
}

// ---------------------------------------------------------------------------
// Expiry cleanup and concurrency
// ---------------------------------------------------------------------------

#[test]
fn cleanup_expired_removes_lookup_and_sense_rows() {
    let (store, clock) = store_with_clock();
    store
        .put_entries("remote-a", "short", &[sample_entry("short")], 10)
        .expect("put short");
    store.put_empty("remote-a", "long", 100).expect("put long");
    store.sense_put("ctx", "{}", 10).expect("sense put");

    clock.advance(20);
    assert_eq!(store.cleanup_expired().expect("cleanup"), 2);
    let stats = store.stats().expect("stats");
    assert_eq!(stats.entries, 1);
    assert_eq!(stats.empty_entries, 1);
    assert_eq!(stats.sense_entries, 0);
    assert_eq!(store.cleanup_expired().expect("cleanup again"), 0);
}

#[test]
fn concurrent_reads_and_writes_do_not_panic() {
    let store = Arc::new(CacheStore::in_memory().expect("open in-memory cache"));
    let mut handles = Vec::new();
    for thread in 0..8 {
        let store = Arc::clone(&store);
        handles.push(std::thread::spawn(move || {
            for iteration in 0..100 {
                let source = format!("source-{}", thread % 3);
                let query = format!("word-{}", iteration % 7);
                store
                    .put_entries(&source, &query, &[sample_entry(&query)], 60)
                    .expect("concurrent put");
                let hit = store.get(&source, &query).expect("concurrent get");
                assert!(hit.is_some(), "row written by the same thread must hit");
                store.stats().expect("concurrent stats");
            }
        }));
    }
    for handle in handles {
        handle.join().expect("worker thread must not panic");
    }

    let stats = store.stats().expect("stats");
    assert!(stats.entries > 0 && stats.entries <= 3 * 7);
    assert_eq!(stats.entries, stats.positive_entries);
}

// ---------------------------------------------------------------------------
// Corruption recovery and shared-file interop
// ---------------------------------------------------------------------------

#[test]
fn corrupt_database_is_quarantined_and_recreated() {
    let dir = temp_dir("corrupt");
    let db_path = dir.join("cache.db");
    std::fs::write(&db_path, b"this is definitely not a sqlite database").expect("write garbage");

    let store = CacheStore::open(&db_path).expect("corrupt db should be recovered");
    assert_eq!(store.path(), Some(db_path.as_path()));
    store
        .put_empty("remote-a", "hello", 60)
        .expect("write into recreated db");

    let backups: Vec<String> = std::fs::read_dir(&dir)
        .expect("read temp dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.starts_with("cache.db.corrupt-"))
        .collect();
    assert_eq!(
        backups.len(),
        1,
        "expected one corrupt backup, got {backups:?}"
    );
    assert!(db_path.exists(), "fresh database should be recreated");
}

#[test]
fn malformed_cached_json_is_a_miss_and_dropped() {
    let dir = temp_dir("bad-json");
    let db_path = dir.join("cache.db");
    {
        let store = CacheStore::open(&db_path).expect("open cache");
        store
            .put_entries("remote-a", "hello", &[sample_entry("hello")], 60)
            .expect("put entries");
    }
    {
        // Simulate a row written by an older/broken build.
        let connection = rusqlite::Connection::open(&db_path).expect("open raw sqlite");
        connection
            .execute("UPDATE lookup_cache SET entries_json = 'not json'", [])
            .expect("corrupt row");
    }

    let store = CacheStore::open(&db_path).expect("reopen cache");
    assert_eq!(store.get("remote-a", "hello").expect("get"), None);
    assert_eq!(
        store.stats().expect("stats").entries,
        0,
        "malformed row must be deleted, not kept"
    );
}

#[test]
fn card_store_and_cache_store_share_one_file() {
    let dir = temp_dir("shared-file");
    let db_path = dir.join("cache.db");

    let cards = CardStore::open(&db_path).expect("open card store");
    cards
        .record_card("hello", Some("Hello world."), "hi", "Mini", 7)
        .expect("record card");

    // Two connections to one file: WAL + busy_timeout + user_version 2 and
    // idempotent schema creation must let either store open first.
    let cache = CacheStore::open(&db_path).expect("open cache store on the same file");
    cache
        .put_entries("remote-a", "hello", &[sample_entry("hello")], 60)
        .expect("put entries");
    assert!(cache.get("remote-a", "hello").expect("get").is_some());

    let cards = CardStore::open(&db_path).expect("reopen card store");
    assert_eq!(cards.count().expect("card count"), 1);
    assert_eq!(cache.stats().expect("stats").entries, 1);
}
