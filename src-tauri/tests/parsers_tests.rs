//! JSONPath 与 QuickJS 响应解析器测试：映射、错误分类、超时与 OOM。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use lexigrab_lib::core::entry::Entry;
use lexigrab_lib::core::error::ParserError;
use lexigrab_lib::parsers::jsonpath::JsonPathParser;
use lexigrab_lib::parsers::response::{
    finalize_entries, ParseContext, RawEntry, ResponseParser, MAX_ENTRIES,
};
use lexigrab_lib::parsers::script::ScriptParser;

static NEXT_SCRIPT: AtomicU32 = AtomicU32::new(0);

fn script_file(source: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-tmp")
        .join(format!(
            "lexigrab-parser-{}-{}",
            std::process::id(),
            NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("parser.js");
    std::fs::write(&path, source).expect("write script");
    path
}

fn parse_script(path: &Path, body: &str, query: &str) -> Result<Vec<Entry>, ParserError> {
    let parser = ScriptParser::new(path.to_path_buf(), Path::new("."), Some(200));
    parser.parse(ParseContext {
        response_body: body,
        query,
    })
}

fn parse_json(expression: &str, body: &str, query: &str) -> Result<Vec<Entry>, ParserError> {
    JsonPathParser::new(expression)?.parse(ParseContext {
        response_body: body,
        query,
    })
}

#[test]
fn jsonpath_standard_and_nested_paths() {
    let body = r#"{"entries":[{"definition_html":"<p>a</p>"},{"definition_html":"<p>b</p>"}]}"#;
    assert_eq!(parse_json("$.entries[*]", body, "q").expect("std").len(), 2);

    let nested = r#"{"data":{"list":[{"headword":"x","definition_html":"<p>a</p>"}]}}"#;
    let entries = parse_json("$.data.list[*]", nested, "q").expect("nested");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].headword, "x");
}

#[test]
fn jsonpath_root_array_hits_are_flattened() {
    let body = r#"[{"definition_html":"<p>a</p>"},{"definition_html":"<p>b</p>"}]"#;
    let entries = parse_json("$[*]", body, "q").expect("root array");
    assert_eq!(entries.len(), 2);
}

#[test]
fn jsonpath_missing_and_bad_fields() {
    let body = r#"{"entries":[{"headword":"a"},7,{"definition_html":"<p>ok</p>","resources":["/rel","https://x/a"]}]}"#;
    let entries = parse_json("$.entries[*]", body, "query").expect("parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].headword, "query");
    assert_eq!(entries[0].resources.len(), 1);

    let error = parse_json(
        "$.entries[*]",
        r#"{"entries":[{"definition_html":1}]}"#,
        "q",
    )
    .expect_err("type error");
    assert!(matches!(error, ParserError::InvalidData(_)));
}

#[test]
fn jsonpath_empty_and_invalid_are_classified() {
    assert!(parse_json("$.entries[*]", r#"{"entries":[]}"#, "q")
        .expect("empty")
        .is_empty());
    assert!(matches!(
        parse_json("$.[", "{}", "q").expect_err("syntax"),
        ParserError::InvalidData(_)
    ));
    assert!(matches!(
        parse_json("$", "{oops", "q").expect_err("json"),
        ParserError::InvalidData(_)
    ));
}

#[test]
fn finalize_caps_entries_and_html_size() {
    let raw: Vec<RawEntry> = (0..MAX_ENTRIES + 5)
        .map(|index| RawEntry {
            definition_html: format!("<p>{index}</p>"),
            ..RawEntry::default()
        })
        .collect();
    assert_eq!(
        finalize_entries(raw, "q").expect("finalize").len(),
        MAX_ENTRIES
    );

    let oversized = RawEntry {
        definition_html: "a".repeat(512 * 1024 + 1),
        ..RawEntry::default()
    };
    assert!(finalize_entries(vec![oversized], "q")
        .expect("finalize")
        .is_empty());
}

#[test]
fn script_maps_entries_end_to_end() {
    let path = script_file(
        r#"
        function parse(ctx) {
          const data = JSON.parse(ctx.responseBody);
          return data.items.map((item) => ({
            headword: item.word,
            definition_html: item.html,
            phonetic: item.phonetic || null,
            resources: item.images || [],
          }));
        }
        "#,
    );
    let body = r#"{"items":[{"word":"hi","html":"<b>hi</b>","phonetic":"/h/","images":["/rel","https://x/a.png"]}]}"#;
    let entries = parse_script(&path, body, "hi").expect("parse");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].headword, "hi");
    assert_eq!(entries[0].definition_html, "<b>hi</b>");
    assert_eq!(entries[0].phonetic.as_deref(), Some("/h/"));
    assert_eq!(entries[0].resources.len(), 1);
    assert!(entries[0].source_name.is_empty());
}

#[test]
fn script_compile_and_runtime_errors_are_invalid_data() {
    let syntax = script_file("function parse( {");
    assert!(matches!(
        parse_script(&syntax, "{}", "q").expect_err("syntax"),
        ParserError::InvalidData(_)
    ));

    let runtime = script_file("function parse() { throw new Error('boom'); }");
    assert!(matches!(
        parse_script(&runtime, "{}", "q").expect_err("runtime"),
        ParserError::InvalidData(_)
    ));

    let missing = script_file("const x = 1;");
    assert!(matches!(
        parse_script(&missing, "{}", "q").expect_err("missing parse"),
        ParserError::InvalidData(_)
    ));
}

#[test]
fn script_dead_loop_times_out_within_400ms() {
    let path = script_file("function parse() { while (true) {} }");
    let started = Instant::now();
    let error = parse_script(&path, "{}", "q").expect_err("timeout");
    assert_eq!(error, ParserError::Timeout);
    assert!(
        started.elapsed() < Duration::from_millis(400),
        "elapsed: {:?}",
        started.elapsed()
    );
}

#[test]
fn script_oom_returns_error_without_crashing() {
    let path = script_file(
        r#"
        function parse() {
          const huge = 'x'.repeat(64 * 1024 * 1024);
          return [{ definition_html: huge }];
        }
        "#,
    );
    let error = parse_script(&path, "{}", "q").expect_err("oom");
    assert!(matches!(error, ParserError::InvalidData(_)), "{error:?}");
}

#[test]
fn script_bad_shape_fetch_and_promise_are_invalid_data() {
    let shape = script_file("function parse() { return { nope: true }; }");
    assert!(matches!(
        parse_script(&shape, "{}", "q").expect_err("shape"),
        ParserError::InvalidData(_)
    ));

    let fetch = script_file("function parse() { fetch('https://example.com'); return []; }");
    assert!(matches!(
        parse_script(&fetch, "{}", "q").expect_err("fetch"),
        ParserError::InvalidData(_)
    ));

    let promise = script_file(
        "function parse() { return Promise.resolve([{ definition_html: '<p>x</p>' }]); }",
    );
    assert!(matches!(
        parse_script(&promise, "{}", "q").expect_err("promise"),
        ParserError::InvalidData(_)
    ));
}
