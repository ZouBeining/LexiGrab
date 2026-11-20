//! M4 性能记录（默认忽略；`cargo test --test perf_tests -- --ignored --nocapture`）。
//!
//! 断言给出宽松上限，真实耗时打印出来写入 `.agents/notes/M4-remote-baseline.md`。
//! 不参与常规 CI，避免机器抖动导致假失败。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use lexigrab_lib::core::config::{SenseSettings, Settings};
use lexigrab_lib::core::entry::{Entry, ResponseParserConfig, SourceConfig, SourceKind};
use lexigrab_lib::core::query_router::QueryRouter;
use lexigrab_lib::core::registry::SourceRegistry;
use lexigrab_lib::core::runtime::NullSink;
use lexigrab_lib::core::sense::select;
use lexigrab_lib::db::cache::CacheStore;
use lexigrab_lib::parsers::jsonpath::JsonPathParser;
use lexigrab_lib::parsers::response::{ParseContext, ResponseParser};
use lexigrab_lib::sources::remote_http::RemoteHttpSource;
use lexigrab_lib::sources::DictionarySource;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn percentile(mut values: Vec<f64>, ratio: f64) -> f64 {
    values.sort_by(|left, right| left.total_cmp(right));
    if values.is_empty() {
        return 0.0;
    }
    let rank = (values.len() as f64 * ratio).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn sample_entry(index: usize) -> Entry {
    let mut entry = Entry::new(
        format!("headword-{index}"),
        format!("<p>definition {index}</p>"),
    );
    entry.source_name = "Perf".to_string();
    entry
}

#[test]
#[ignore = "performance probe"]
fn cache_lookup_p95_under_5ms() {
    let cache = CacheStore::in_memory().expect("cache");
    for index in 0..200 {
        cache
            .put_entries(
                "remote-1",
                &format!("word-{index}"),
                &[sample_entry(index)],
                3600,
            )
            .expect("seed");
    }
    let mut samples = Vec::with_capacity(1000);
    for index in 0..1000 {
        let word = format!("word-{}", index % 200);
        let started = Instant::now();
        let hit = cache.get("remote-1", &word).expect("get");
        assert!(hit.is_some());
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let p95 = percentile(samples, 0.95);
    println!("cache.get p95 = {p95:.3} ms");
    assert!(p95 < 5.0, "cache p95 {p95} ms 超过 5ms");
}

#[test]
#[ignore = "performance probe"]
fn jsonpath_100_entries_under_10ms() {
    let items: Vec<serde_json::Value> = (0..100)
        .map(|index| json!({"headword": format!("w{index}"), "definition_html": format!("<p>{index}</p>")}))
        .collect();
    let body = json!({"entries": items}).to_string();
    let parser = JsonPathParser::new("$.entries[*]").expect("parser");
    let mut samples = Vec::new();
    for _ in 0..50 {
        let started = Instant::now();
        let entries = parser
            .parse(ParseContext {
                response_body: &body,
                query: "w",
            })
            .expect("parse");
        assert_eq!(entries.len(), 20, "每查询最多 20 条");
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let p95 = percentile(samples, 0.95);
    println!("jsonpath 100 entries p95 = {p95:.3} ms");
    assert!(p95 < 10.0, "jsonpath p95 {p95} ms 超过 10ms");
}

#[test]
fn remote_timeout_with_one_retry_stays_within_contract() {
    // 契约 T4.1：超时最多重试 1 次、间隔 150ms。因此最坏耗时约为
    // 2 × timeout + 150ms；计划里的“配置 + 200ms”只适用于单次尝试。
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(async { MockServer::start().await });
    runtime.block_on(async {
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(3)))
            .mount(&server)
            .await;
    });
    let config = SourceConfig {
        id: "remote-perf".to_string(),
        name: "Slow".to_string(),
        enabled: true,
        priority: 10,
        kind: SourceKind::RemoteHttp {
            url_template: format!("{}/slow?q={{word}}", server.uri()),
            response_parser: ResponseParserConfig::Jsonpath {
                expression: "$.entries[*]".to_string(),
            },
            auth_header: None,
            auth_value: None,
            timeout_ms: 300,
        },
    };
    let source = Arc::new(RemoteHttpSource::open(config, Path::new(".")));
    let started = Instant::now();
    let result = std::thread::spawn(move || source.lookup("bank"))
        .join()
        .expect("join");
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    println!("remote timeout elapsed = {elapsed:.1} ms (config 300ms, retry once)");
    assert!(result.is_err());
    assert!(
        elapsed <= 800.0,
        "远程超时（含一次重试）{elapsed} ms 超过 2×配置 + 200ms"
    );
}

#[test]
fn selector_call_stays_under_800ms() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(async { MockServer::start().await });
    runtime.block_on(async {
        Mock::given(method("POST"))
            .and(path("/v1/select"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "selected_id": "e0-s1",
                "confidence": 0.9,
                "model": "perf-mock"
            })))
            .mount(&server)
            .await;
    });
    let settings = SenseSettings {
        enabled: true,
        selector_endpoint: format!("{}/v1/select", server.uri()),
        selector_timeout_ms: 800,
        ..SenseSettings::default()
    };
    let senses = lexigrab_lib::parsers::sense::extract_senses(
        &sample_entry(0),
        0,
        settings.max_senses as usize,
    );
    let started = Instant::now();
    let reply = select(&settings, "bank", "He sat on the bank.", &senses).expect("select");
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    println!("selector overhead = {elapsed:.1} ms");
    assert_eq!(reply.model.as_deref(), Some("perf-mock"));
    assert!(elapsed <= 800.0, "选择器额外耗时 {elapsed} ms 超过 800ms");
}

#[test]
fn aggregate_does_not_wait_for_remote_sources() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let server = runtime.block_on(async { MockServer::start().await });
    runtime.block_on(async {
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(2)))
            .mount(&server)
            .await;
    });
    let config = SourceConfig {
        id: "remote-slow".to_string(),
        name: "Slow".to_string(),
        enabled: true,
        priority: 100,
        kind: SourceKind::RemoteHttp {
            url_template: format!("{}/slow?q={{word}}", server.uri()),
            response_parser: ResponseParserConfig::Jsonpath {
                expression: "$.entries[*]".to_string(),
            },
            auth_header: None,
            auth_value: None,
            timeout_ms: 3000,
        },
    };
    let mut registry = SourceRegistry::empty();
    registry.register(
        config.clone(),
        Arc::new(RemoteHttpSource::open(config, Path::new("."))),
    );
    let settings = Settings::default();
    let router = QueryRouter::new(&registry, &settings);
    let started = Instant::now();
    let outcome = router
        .lookup_planned("bank", None, "q", &NullSink)
        .expect("lookup");
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "local_first first screen = {elapsed:.1} ms, pending remote = {}",
        outcome.remote_jobs.len()
    );
    assert_eq!(outcome.remote_jobs.len(), 1);
    assert!(elapsed < 200.0, "首屏不应等待远程源，实际 {elapsed} ms");
}
