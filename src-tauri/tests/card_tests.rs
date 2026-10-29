//! Integration tests for the saved-card SQLite store, context fingerprints and
//! the Anki export pipeline. Contract: docs/BACKEND_STRUCTURE.md §3 and §6.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{json, Value};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use lexigrab_lib::anki::export::{export_to_anki, strip_local_media, ExportDeps, ExportOutcome};
use lexigrab_lib::core::config::{AnkiSettings, DuplicatePolicy};
use lexigrab_lib::core::entry::AnkiPayload;
use lexigrab_lib::core::error::AnkiError;
use lexigrab_lib::db::CardStore;

/// Isolated temp dir under `src-tauri/target/test-tmp` (never `%TEMP%`).
fn temp_dir(tag: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("test-tmp")
        .join("card-tests")
        .join(tag);
    if root.exists() {
        let _ = std::fs::remove_dir_all(&root);
    }
    std::fs::create_dir_all(&root).expect("create temp dir");
    root
}

// ---------------------------------------------------------------------------
// CardStore context checks
// ---------------------------------------------------------------------------

#[test]
fn same_word_sentence_definition_is_seen_with_note_id() {
    let store = CardStore::in_memory().expect("open store");
    assert_eq!(store.count().expect("count"), 0);

    store
        .record_card("Hello", Some("Hello world."), "hi", "Mini", 1001)
        .expect("record card");

    // Normalization: case and whitespace are ignored; definition arguments are
    // already plain text (callers run `definition_text` on the HTML first).
    let context = store
        .check_context("  hello ", Some(" hello   world. "), "  Hi ")
        .expect("check context");
    assert!(context.seen);
    assert_eq!(context.note_id, Some(1001));
    assert!(context.created_at.is_some());
    assert_eq!(store.count().expect("count"), 1);
}

#[test]
fn different_sentence_is_not_seen() {
    let store = CardStore::in_memory().expect("open store");
    store
        .record_card("hello", Some("first sentence"), "hi", "Mini", 1)
        .expect("record card");

    let context = store
        .check_context("hello", Some("second sentence"), "hi")
        .expect("check context");
    assert!(!context.seen);
    assert_eq!(context.note_id, None);
    assert_eq!(context.created_at, None);
}

#[test]
fn same_sentence_different_definition_is_not_seen() {
    let store = CardStore::in_memory().expect("open store");
    store
        .record_card("hello", Some("same sentence"), "first", "Mini", 1)
        .expect("record card");

    let context = store
        .check_context("hello", Some("same sentence"), "second")
        .expect("check context");
    assert!(!context.seen);
}

#[test]
fn absent_and_blank_sentence_share_one_context() {
    let store = CardStore::in_memory().expect("open store");
    store
        .record_card("hello", None, "hi", "Mini", 7)
        .expect("record card");

    let context = store
        .check_context("hello", Some("   "), "hi")
        .expect("check context");
    assert!(context.seen);
    assert_eq!(context.note_id, Some(7));
}

#[test]
fn corrupt_database_is_quarantined_and_recreated() {
    let dir = temp_dir("corrupt");
    let db_path = dir.join("cache.db");
    std::fs::write(&db_path, b"this is definitely not a sqlite database").expect("write garbage");

    let store = CardStore::open(&db_path).expect("corrupt db should be recovered");
    assert_eq!(store.count().expect("count"), 0);
    store
        .record_card("hello", None, "hi", "Mini", 5)
        .expect("record card into recreated db");

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
fn open_creates_missing_parent_directories() {
    let dir = temp_dir("nested");
    let db_path = dir.join("a").join("b").join("cache.db");
    let store = CardStore::open(&db_path).expect("open store");
    assert_eq!(store.count().expect("count"), 0);
    assert!(db_path.exists());
}

// ---------------------------------------------------------------------------
// Media stripping
// ---------------------------------------------------------------------------

#[test]
fn strip_local_media_replaces_local_elements_with_placeholders() {
    let html = concat!(
        r#"<p>see <img src="mdx-res://local-1/img/a.png"> here</p>"#,
        r#"<audio src="http://mdx-res.localhost/local-1/a.mp3"></audio>"#,
        r#"<video src="mdx-res://local-1/v.mp4" poster="mdx-res://local-1/p.png"></video>"#,
        r#"<img src="https://example.com/remote.png">"#,
    );

    let stripped = strip_local_media(html);
    assert!(!stripped.contains("mdx-res"), "stripped output: {stripped}");
    assert!(!stripped.contains("mdx-res.localhost"));
    assert!(stripped.contains("[图片]"), "stripped output: {stripped}");
    assert!(stripped.contains("[音频]"), "stripped output: {stripped}");
    assert!(stripped.contains("[视频]"), "stripped output: {stripped}");
    // Remote media is preserved.
    assert!(
        stripped.contains("https://example.com/remote.png"),
        "stripped output: {stripped}"
    );
}

#[test]
fn strip_local_media_drops_local_source_children() {
    let html = r#"<audio controls><source src="mdx-res://local-1/a.ogg" type="audio/ogg"></audio>"#;
    let stripped = strip_local_media(html);
    assert!(!stripped.contains("mdx-res"), "stripped output: {stripped}");
}

// ---------------------------------------------------------------------------
// Export pipeline against a mock AnkiConnect server
// ---------------------------------------------------------------------------

fn settings_for(server: &MockServer) -> AnkiSettings {
    AnkiSettings {
        endpoint: server.uri(),
        ..AnkiSettings::default()
    }
}

/// Mount the `modelFieldNames` and `addNote` endpoints with exact request bodies.
async fn mock_anki_export(fields: Value, expected_note: Value, note_id: i64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({
            "action": "modelFieldNames",
            "version": 6,
            "params": { "modelName": "Basic" },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": fields,
            "error": null,
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({
            "action": "addNote",
            "version": 6,
            "params": { "note": expected_note },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": note_id,
            "error": null,
        })))
        .mount(&server)
        .await;
    server
}

/// Expected addNote note body for the sample payload, including `allowDuplicate`.
fn expected_note(definition: &str) -> Value {
    json!({
        "deckName": "Default",
        "modelName": "Basic",
        "fields": {
            "Word": "hello",
            "Definition": definition,
            "Sentence": "Hello world.",
            "Source": "Mini",
        },
        "tags": ["global-dict"],
        "options": { "allowDuplicate": true },
    })
}

/// Run the blocking export off the async runtime.
async fn run_export(
    settings: AnkiSettings,
    cards: Arc<CardStore>,
    payload: AnkiPayload,
) -> Result<ExportOutcome, AnkiError> {
    tokio::task::spawn_blocking(move || {
        export_to_anki(
            ExportDeps {
                settings: &settings,
                cards: &cards,
            },
            &payload,
        )
    })
    .await
    .expect("spawn_blocking join")
}

fn sample_payload() -> AnkiPayload {
    AnkiPayload::new(
        "hello",
        r#"<b>hi</b><img src="mdx-res://local-1/img/a.png">"#,
        Some("Hello world.".to_string()),
        "Mini",
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn export_reports_context_on_second_identical_export() {
    let payload = sample_payload();
    let definition = strip_local_media(&payload.definition_html);
    assert!(definition.contains("[图片]"));
    let server = mock_anki_export(
        json!(["Word", "Definition", "Sentence", "Source"]),
        expected_note(&definition),
        1234567890,
    )
    .await;
    let settings = settings_for(&server);
    let cards = Arc::new(CardStore::in_memory().expect("open store"));

    let first = run_export(settings.clone(), Arc::clone(&cards), payload.clone())
        .await
        .expect("first export should succeed");
    assert_eq!(first.note_id, 1234567890);
    assert!(!first.exact_context_seen);

    let second = run_export(settings, Arc::clone(&cards), payload.clone())
        .await
        .expect("second export should succeed");
    assert_eq!(second.note_id, 1234567890);
    assert!(second.exact_context_seen);

    assert_eq!(cards.count().expect("count"), 2);

    // Every addNote request must carry allowDuplicate: true.
    let requests = server.received_requests().await.expect("recorded requests");
    let mut add_notes = 0;
    for request in requests {
        let body: Value = serde_json::from_slice(&request.body).expect("request body json");
        if body["action"] == json!("addNote") {
            add_notes += 1;
            assert_eq!(
                body["params"]["note"]["options"]["allowDuplicate"],
                json!(true)
            );
        }
    }
    assert_eq!(add_notes, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn export_with_allow_policy_skips_context_check() {
    let payload = sample_payload();
    let definition = strip_local_media(&payload.definition_html);
    let server = mock_anki_export(
        json!(["Word", "Definition", "Sentence", "Source"]),
        expected_note(&definition),
        42,
    )
    .await;
    let mut settings = settings_for(&server);
    settings.duplicate_policy = DuplicatePolicy::Allow;
    let cards = Arc::new(CardStore::in_memory().expect("open store"));

    let first = run_export(settings.clone(), Arc::clone(&cards), payload.clone())
        .await
        .expect("first export should succeed");
    assert!(!first.exact_context_seen);
    let second = run_export(settings, Arc::clone(&cards), payload.clone())
        .await
        .expect("second export should succeed");
    // Policy Allow never checks the store, so the flag stays false even though
    // the first export recorded the same context.
    assert!(!second.exact_context_seen);
    assert_eq!(cards.count().expect("count"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn export_reports_missing_field_as_anki_field_missing() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({
            "action": "modelFieldNames",
            "version": 6,
            "params": { "modelName": "Basic" },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": ["Word", "Sentence", "Source"],
            "error": null,
        })))
        .mount(&server)
        .await;
    let settings = settings_for(&server);
    let cards = Arc::new(CardStore::in_memory().expect("open store"));

    let error = match run_export(settings, cards, sample_payload()).await {
        Ok(outcome) => panic!("expected FieldMissing, got {outcome:?}"),
        Err(error) => error,
    };
    assert_eq!(error, AnkiError::FieldMissing("Definition".to_string()));
    assert_eq!(error.code(), "ANKI_FIELD_MISSING");
}
