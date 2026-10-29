//! MDX 解析器与 worker 集成测试，使用提交的夹具。

use std::path::{Path, PathBuf};
use std::time::Duration;

use lexigrab_lib::core::entry::ResourceRef;
use lexigrab_lib::core::error::ParserError;
use lexigrab_lib::parsers::mdx::{normalize_resource_path, MdxParser};
use lexigrab_lib::parsers::{FormatParser, ResourceProvider};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn open_mini() -> MdxParser {
    let mut parser = MdxParser::new("local-mini-f1");
    parser
        .open_with_timeout(&fixture("mini.mdx"), Duration::from_secs(15))
        .expect("open mini.mdx");
    parser
}

#[test]
fn opens_fixture_and_reports_entry_count() {
    let parser = open_mini();
    assert_eq!(parser.entry_count(), 3);
    assert!(parser.can_parse(&fixture("mini.mdx")));
    assert!(!parser.can_parse(&fixture("mini.mdd")));
}

#[test]
fn lookup_returns_entries_with_rewritten_resources() {
    let parser = open_mini();
    let entries = parser.lookup("hello").expect("lookup hello");
    assert!(!entries.is_empty());
    assert!(entries.len() <= 3);
    let entry = &entries[0];
    assert_eq!(entry.headword, "hello");
    assert_eq!(entry.source_name, "");
    assert!(
        entry.definition_html.contains("mdx-res://local-mini-f1/"),
        "html: {}",
        entry.definition_html
    );
    assert!(!entry.definition_html.contains("<script"));
    assert!(entry.resources.iter().any(|resource| matches!(
        resource,
        ResourceRef::Local { path, .. } if path == "img/hello.png"
    )));
}

#[test]
fn lookup_is_case_tolerant() {
    let parser = open_mini();
    for query in ["hello", "Hello", "HELLO"] {
        let entries = parser.lookup(query).expect("lookup");
        assert!(!entries.is_empty(), "query {query} should resolve");
    }
}

#[test]
fn lookup_normalizes_surrounding_punctuation() {
    let parser = open_mini();
    let entries = parser.lookup("“hello”").expect("lookup");
    assert!(!entries.is_empty());
}

#[test]
fn missing_word_returns_empty() {
    let parser = open_mini();
    assert!(parser
        .lookup("definitely-not-in-dictionary")
        .expect("lookup")
        .is_empty());
}

#[test]
fn resolves_image_and_audio_resources() {
    let parser = open_mini();
    let image = parser.resolve("img/hello.png").expect("image resource");
    assert!(image.bytes.starts_with(b"\x89PNG"));
    assert_eq!(image.mime, "image/png");

    let audio = parser.resolve("audio/hello.mp3").expect("audio resource");
    assert_eq!(audio.mime, "audio/mpeg");

    let backslash = parser.resolve("\\img\\hello.png").expect("backslash path");
    assert_eq!(backslash.bytes, image.bytes);
}

#[test]
fn resource_path_traversal_is_rejected() {
    let parser = open_mini();
    let error = parser.resolve("../secret").expect_err("traversal");
    assert!(matches!(error, ParserError::ResourceNotFound(_)));
    assert!(normalize_resource_path("../secret").is_err());
    assert!(parser.resolve("img/missing.png").is_err());
}

#[test]
fn truncated_fixture_fails_to_open() {
    let mut parser = MdxParser::new("broken");
    let error = parser
        .open_with_timeout(&fixture("broken.mdx"), Duration::from_secs(5))
        .expect_err("truncated file should fail");
    assert!(matches!(error, ParserError::Open(_)), "error: {error:?}");
}

#[test]
fn closed_worker_returns_worker_dead() {
    let mut parser = open_mini();
    parser.close();
    let error = parser.lookup("hello").expect_err("dead worker");
    assert_eq!(error, ParserError::WorkerDead);
}

#[test]
fn missing_file_reports_open_error() {
    let mut parser = MdxParser::new("missing");
    let error = parser
        .open_with_timeout(&fixture("does-not-exist.mdx"), Duration::from_secs(5))
        .expect_err("missing file");
    assert!(matches!(error, ParserError::Open(_)));
}
