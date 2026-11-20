// SPDX-License-Identifier: AGPL-3.0-only
//! 远程 HTTP 数据源集成测试：wiremock 覆盖状态码、超时、重定向、上限、鉴权与端到端解析。
//!
//! reqwest blocking 客户端会自带 tokio runtime，构造与查询都必须在 `std::thread` 中完成。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use lexigrab_lib::core::entry::{Entry, ResponseParserConfig, SourceConfig, SourceKind};
use lexigrab_lib::core::error::SourceError;
use lexigrab_lib::sources::remote_http::{
    test_query, RemoteHttpSource, RemoteTestError, RemoteTestOutcome,
};
use lexigrab_lib::sources::{DictionarySource, SourceStatus};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

static NEXT_SCRIPT: AtomicU32 = AtomicU32::new(0);

fn jsonpath_config(template: String, timeout_ms: u64) -> SourceConfig {
    SourceConfig {
        id: "remote-test".to_string(),
        name: "Remote".to_string(),
        enabled: true,
        priority: 10,
        kind: SourceKind::RemoteHttp {
            url_template: template,
            response_parser: ResponseParserConfig::Jsonpath {
                expression: "$.entries[*]".to_string(),
            },
            auth_header: None,
            auth_value: None,
            timeout_ms,
        },
    }
}

fn script_config(template: String, script: PathBuf) -> SourceConfig {
    SourceConfig {
        id: "remote-script".to_string(),
        name: "Remote Script".to_string(),
        enabled: true,
        priority: 10,
        kind: SourceKind::RemoteHttp {
            url_template: template,
            response_parser: ResponseParserConfig::Script {
                path: script,
                timeout_ms: Some(200),
            },
            auth_header: None,
            auth_value: None,
            timeout_ms: 3000,
        },
    }
}

fn script_file(source: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-tmp")
        .join(format!(
            "lexigrab-remote-{}-{}",
            std::process::id(),
            NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("remote.js");
    std::fs::write(&path, source).expect("write script");
    path
}

/// 在 blocking 线程里构造数据源并查询，避免 async 上下文里创建 tokio runtime。
fn run_lookup(
    config: SourceConfig,
    query: &str,
) -> (SourceStatus, Result<Vec<Entry>, SourceError>) {
    let query = query.to_string();
    std::thread::spawn(move || {
        let source = RemoteHttpSource::open(config, Path::new("."));
        let status = source.status();
        let result = source.lookup(&query);
        (status, result)
    })
    .join()
    .expect("lookup thread")
}

fn run_test_query(config: SourceConfig, query: &str) -> Result<RemoteTestOutcome, RemoteTestError> {
    let query = query.to_string();
    std::thread::spawn(move || test_query(&config, Path::new("."), &query))
        .join()
        .expect("test query thread")
}

const ENTRIES_BODY: &str = r#"{"entries":[{"headword":"hello","definition_html":"<p>hi</p>","resources":["https://x/a.png"]}]}"#;

#[tokio::test]
async fn jsonpath_success_fills_source_name_and_filters_relative_resources() {
    let server = MockServer::start().await;
    let body = r#"{"entries":[
        {"headword":"hello","definition_html":"<p>hi</p>","resources":["/rel.png","https://x/a.png"]},
        {"headword":"no-definition"}
    ]}"#;
    Mock::given(method("GET"))
        .and(path("/lookup/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let (status, result) = run_lookup(
        jsonpath_config(format!("{}/lookup/{{word}}", server.uri()), 3000),
        "hello",
    );
    assert!(status.is_ready());
    let entries = result.expect("lookup");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].source_name, "Remote");
    assert_eq!(entries[0].resources.len(), 1);
}

#[tokio::test]
async fn unauthorized_and_server_errors_are_unavailable_and_not_retried() {
    for status in [401u16, 500u16] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/lookup/hello"))
            .respond_with(ResponseTemplate::new(status).set_body_string("nope"))
            .mount(&server)
            .await;

        let (_, result) = run_lookup(
            jsonpath_config(format!("{}/lookup/{{word}}", server.uri()), 3000),
            "hello",
        );
        let error = result.expect_err("http error");
        assert!(matches!(error, SourceError::Unavailable(_)), "{error:?}");
        assert_eq!(
            server.received_requests().await.expect("requests").len(),
            1,
            "HTTP {status} 不应重试"
        );
    }
}

#[tokio::test]
async fn timeout_retries_once_and_maps_to_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/lookup/hello"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ENTRIES_BODY)
                .set_delay(Duration::from_millis(700)),
        )
        .mount(&server)
        .await;

    let started = Instant::now();
    let (_, result) = run_lookup(
        jsonpath_config(format!("{}/lookup/{{word}}", server.uri()), 200),
        "hello",
    );
    assert_eq!(result.expect_err("timeout"), SourceError::Timeout);
    assert_eq!(
        server.received_requests().await.expect("requests").len(),
        2,
        "超时应重试一次"
    );
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn cross_host_redirect_is_not_followed() {
    let target = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/target"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ENTRIES_BODY))
        .mount(&target)
        .await;

    let origin = MockServer::start().await;
    // 用 localhost 替换 127.0.0.1，制造不同主机名（端口仍指向同一 mock）。
    let location = format!("{}/target", target.uri()).replace("127.0.0.1", "localhost");
    Mock::given(method("GET"))
        .and(path("/lookup/hello"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", location))
        .mount(&origin)
        .await;

    let (_, result) = run_lookup(
        jsonpath_config(format!("{}/lookup/{{word}}", origin.uri()), 3000),
        "hello",
    );
    let error = result.expect_err("redirect stopped");
    assert!(matches!(error, SourceError::Unavailable(_)), "{error:?}");
    assert_eq!(
        target.received_requests().await.expect("requests").len(),
        0,
        "跨主机重定向不应被跟随"
    );
}

#[tokio::test]
async fn oversized_response_is_rejected() {
    let server = MockServer::start().await;
    let body = vec![b'a'; 2 * 1024 * 1024 + 1];
    Mock::given(method("GET"))
        .and(path("/lookup/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(&server)
        .await;

    let (_, result) = run_lookup(
        jsonpath_config(format!("{}/lookup/{{word}}", server.uri()), 3000),
        "hello",
    );
    let error = result.expect_err("too large");
    assert!(matches!(error, SourceError::Unavailable(_)), "{error:?}");
}

#[tokio::test]
async fn auth_header_is_sent() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/auth/hello"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ENTRIES_BODY))
        .mount(&server)
        .await;

    let mut config = jsonpath_config(format!("{}/auth/{{word}}", server.uri()), 3000);
    if let SourceKind::RemoteHttp {
        auth_header,
        auth_value,
        ..
    } = &mut config.kind
    {
        *auth_header = Some("authorization".to_string());
        *auth_value = Some("Bearer secret".to_string());
    }
    let (_, result) = run_lookup(config, "hello");
    assert_eq!(result.expect("auth lookup").len(), 1);
}

#[tokio::test]
async fn query_word_is_percent_encoded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "hello world&x"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ENTRIES_BODY))
        .mount(&server)
        .await;

    let (_, result) = run_lookup(
        jsonpath_config(format!("{}/search?q={{word}}", server.uri()), 3000),
        "hello world&x",
    );
    assert_eq!(result.expect("encoded query").len(), 1);
}

#[tokio::test]
async fn script_parser_end_to_end() {
    let server = MockServer::start().await;
    let body = r#"{"items":[{"word":"hi","html":"<b>hi</b>"}]}"#;
    Mock::given(method("GET"))
        .and(path("/script/hi"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let script = script_file(
        r#"
        function parse(ctx) {
          const data = JSON.parse(ctx.responseBody);
          return data.items.map((item) => ({
            headword: item.word,
            definition_html: item.html,
            resources: [],
          }));
        }
        "#,
    );
    let (_, result) = run_lookup(
        script_config(format!("{}/script/{{word}}", server.uri()), script),
        "hi",
    );
    let entries = result.expect("script lookup");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].definition_html, "<b>hi</b>");
    assert_eq!(entries[0].source_name, "Remote Script");
}

#[tokio::test]
async fn invalid_config_marks_source_failed() {
    for kind in [
        SourceKind::RemoteHttp {
            url_template: "ftp://example.com/{word}".to_string(),
            response_parser: ResponseParserConfig::Jsonpath {
                expression: "$.entries[*]".to_string(),
            },
            auth_header: None,
            auth_value: None,
            timeout_ms: 3000,
        },
        SourceKind::RemoteHttp {
            url_template: format!("{}/{{word}}/{{word}}", "http://example.com"),
            response_parser: ResponseParserConfig::Jsonpath {
                expression: "$.entries[*]".to_string(),
            },
            auth_header: Some("authorization".to_string()),
            auth_value: None,
            timeout_ms: 3000,
        },
    ] {
        let config = SourceConfig {
            id: "remote-bad".to_string(),
            name: "Bad".to_string(),
            enabled: true,
            priority: 10,
            kind,
        };
        let (status, result) = run_lookup(config, "hello");
        assert!(matches!(status, SourceStatus::Failed { .. }));
        let error = result.expect_err("init error");
        assert!(matches!(error, SourceError::Unavailable(_)), "{error:?}");
    }
}

#[tokio::test]
async fn status_tracks_last_error_and_clears_on_success() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky/fail"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/flaky/ok"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ENTRIES_BODY))
        .mount(&server)
        .await;

    let config = jsonpath_config(format!("{}/flaky/{{word}}", server.uri()), 3000);
    let (initial, after_fail, after_ok, failure, success) = std::thread::spawn(move || {
        let source = RemoteHttpSource::open(config, Path::new("."));
        let initial = source.status();
        let failure = source.lookup("fail");
        let after_fail = source.status();
        let success = source.lookup("ok");
        let after_ok = source.status();
        (initial, after_fail, after_ok, failure, success)
    })
    .join()
    .expect("status thread");

    assert!(initial.is_ready());
    match after_fail {
        SourceStatus::Failed { message } => assert!(message.contains("HTTP 500"), "{message}"),
        other => panic!("expected failed status, got {other:?}"),
    }
    assert!(matches!(failure, Err(SourceError::Unavailable(_))));
    assert!(after_ok.is_ready());
    assert_eq!(success.expect("recovered lookup").len(), 1);
}

#[tokio::test]
async fn test_query_returns_preview_on_success_and_failure() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ENTRIES_BODY))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/bad/hello"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server exploded"))
        .mount(&server)
        .await;

    let ok = run_test_query(
        jsonpath_config(format!("{}/ok/{{word}}", server.uri()), 3000),
        "hello",
    )
    .expect("test query ok");
    assert_eq!(ok.entries.len(), 1);
    assert_eq!(ok.entries[0].source_name, "Remote");
    assert!(ok
        .raw_preview
        .as_deref()
        .is_some_and(|preview| preview.contains("\"entries\"")));

    let failure = run_test_query(
        jsonpath_config(format!("{}/bad/{{word}}", server.uri()), 3000),
        "hello",
    )
    .expect_err("test query failure");
    assert_eq!(failure.error.code(), "DICT_NOT_READY");
    assert_eq!(failure.raw_preview.as_deref(), Some("server exploded"));
}
