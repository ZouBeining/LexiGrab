//! 语境辅助：HTTP 客户端（wiremock）与状态机（fake 选择器/LLM）测试。
//!
//! HTTP 用例把阻塞客户端放进 `spawn_blocking`，与 `anki_tests.rs` 的做法一致；
//! 状态机用例使用 fake trait 实现，只验证阈值、缓存与降级顺序。
//! 被远程限制阻止的端点不会产生请求，用 wiremock 的请求记录断言。

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use lexigrab_lib::core::config::{LlmSettings, SenseSettings, Settings};
use lexigrab_lib::core::entry::Entry;
use lexigrab_lib::core::sense::{
    generate, resolve_with, select, AiDefinition, LlmError, LlmGenerator, SelectorReply,
    SenseCandidate, SenseError, SenseInput, SenseResolution, SenseSelector, SenseStatus,
};
use lexigrab_lib::db::cache::{CacheStore, Clock};

const SENTENCE: &str = "He sat on the bank of the river.";

// ---------------------------------------------------------------------------
// 公共夹具
// ---------------------------------------------------------------------------

fn candidate(id: &str, definition: &str) -> SenseCandidate {
    SenseCandidate {
        id: id.to_string(),
        entry_index: 0,
        source_name: "Mini".to_string(),
        headword: "bank".to_string(),
        part_of_speech: Some("noun".to_string()),
        definition_text: definition.to_string(),
        definition_html: format!("<li>{definition}</li>"),
    }
}

fn sample_senses() -> Vec<SenseCandidate> {
    vec![
        candidate("e0-s1", "financial institution"),
        candidate("e0-s2", "sloping land beside water"),
    ]
}

fn selector_settings(endpoint: String, timeout_ms: u64) -> SenseSettings {
    SenseSettings {
        enabled: true,
        selector_endpoint: endpoint,
        selector_timeout_ms: timeout_ms,
        ..SenseSettings::default()
    }
}

fn llm_settings(endpoint: String, timeout_ms: u64) -> LlmSettings {
    LlmSettings {
        enabled: true,
        endpoint,
        model: "test-model".to_string(),
        timeout_ms,
        max_tokens: 64,
        ..LlmSettings::default()
    }
}

fn endpoint(server: &MockServer, route: &str) -> String {
    format!("{}{route}", server.uri())
}

fn selector_request_body() -> Value {
    json!({
        "word": "bank",
        "sentence": SENTENCE,
        "locale": "en",
        "senses": [
            {
                "id": "e0-s1",
                "part_of_speech": "noun",
                "definition": "financial institution",
            },
            {
                "id": "e0-s2",
                "part_of_speech": "noun",
                "definition": "sloping land beside water",
            },
        ],
    })
}

async fn run_select(
    settings: SenseSettings,
    senses: Vec<SenseCandidate>,
) -> Result<SelectorReply, SenseError> {
    tokio::task::spawn_blocking(move || select(&settings, "bank", SENTENCE, &senses))
        .await
        .expect("选择器任务应当完成")
}

async fn run_generate(
    settings: LlmSettings,
    senses: Vec<SenseCandidate>,
    based_on: Option<String>,
) -> Result<AiDefinition, LlmError> {
    tokio::task::spawn_blocking(move || {
        generate(&settings, "bank", SENTENCE, &senses, based_on.as_deref())
    })
    .await
    .expect("LLM 任务应当完成")
}

fn llm_response(definition: &str) -> ResponseTemplate {
    let content = json!({
        "definition": definition,
        "part_of_speech": "noun",
        "confidence": 0.7,
        "explanation": "the sentence mentions the river",
    })
    .to_string();
    ResponseTemplate::new(200).set_body_json(json!({
        "choices": [{ "message": { "role": "assistant", "content": content } }],
    }))
}

// ---------------------------------------------------------------------------
// 选择器 HTTP 客户端
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn http_selector_posts_contract_and_parses_reply() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/select"))
        .and(body_json(selector_request_body()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "selected_id": "e0-s2",
            "confidence": 0.81,
            "distribution": { "e0-s1": 0.19, "e0-s2": 0.81 },
            "model": "jev-like-2026-10",
            "reason": "river context",
        })))
        .mount(&server)
        .await;

    let reply = run_select(
        selector_settings(endpoint(&server, "/v1/select"), 500),
        sample_senses(),
    )
    .await
    .expect("选择器应成功");
    assert_eq!(reply.selected_id.as_deref(), Some("e0-s2"));
    assert!((reply.confidence.expect("置信度") - 0.81).abs() < 1e-6);
    assert_eq!(reply.model.as_deref(), Some("jev-like-2026-10"));
    assert!(reply.distribution.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn http_selector_passes_unknown_selected_id_through() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "selected_id": "e9-s9",
            "confidence": 0.9,
        })))
        .mount(&server)
        .await;

    let reply = run_select(
        selector_settings(endpoint(&server, "/v1/select"), 500),
        sample_senses(),
    )
    .await
    .expect("选择器应成功");
    // 合法性由 resolution 判定，HTTP 适配层原样返回。
    assert_eq!(reply.selected_id.as_deref(), Some("e9-s9"));
}

#[tokio::test(flavor = "multi_thread")]
async fn http_selector_timeout_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(800))
                .set_body_json(json!({ "selected_id": "e0-s1", "confidence": 0.9 })),
        )
        .mount(&server)
        .await;

    let error = run_select(
        selector_settings(endpoint(&server, "/v1/select"), 200),
        sample_senses(),
    )
    .await
    .expect_err("应当超时");
    assert_eq!(error, SenseError::Timeout);
}

#[tokio::test(flavor = "multi_thread")]
async fn http_selector_non_200_is_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let error = run_select(
        selector_settings(endpoint(&server, "/v1/select"), 500),
        sample_senses(),
    )
    .await
    .expect_err("应返回失败");
    assert!(
        matches!(error, SenseError::Failed(_)),
        "实际错误: {error:?}"
    );
    assert_eq!(error.code(), "SENSE_SELECTOR_FAILED");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_selector_blocks_remote_endpoint_without_sending() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "selected_id": "e0-s1" })))
        .mount(&server)
        .await;
    // 用 0.0.0.0 伪装成非回环主机；若代码漏掉门禁，请求可能落到同一端口的 mock 上。
    let blocked = format!("{}/v1/select", server.uri().replace("127.0.0.1", "0.0.0.0"));

    let error = run_select(selector_settings(blocked, 500), sample_senses())
        .await
        .expect_err("应被阻止");
    assert_eq!(error, SenseError::RemoteBlocked);
    let received = server
        .received_requests()
        .await
        .expect("wiremock 应记录请求");
    assert!(received.is_empty(), "被阻止的端点不应发出请求");
}

// ---------------------------------------------------------------------------
// LLM HTTP 客户端
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_sends_openai_contract_and_parses_definition() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(llm_response("the land alongside a river"))
        .mount(&server)
        .await;

    let ai = run_generate(
        llm_settings(endpoint(&server, "/v1/chat/completions"), 1000),
        sample_senses(),
        Some("e0-s1".to_string()),
    )
    .await
    .expect("LLM 应成功");
    assert_eq!(ai.definition_text, "the land alongside a river");
    assert_eq!(ai.part_of_speech.as_deref(), Some("noun"));
    assert_eq!(ai.model, "test-model");
    assert_eq!(ai.based_on_sense_id.as_deref(), Some("e0-s1"));
    assert!(ai.confidence.expect("置信度") > 0.0);
    assert!(ai.generated_at > 0);

    let received = server
        .received_requests()
        .await
        .expect("wiremock 应记录请求");
    assert_eq!(received.len(), 1);
    let body: Value = serde_json::from_slice(&received[0].body).expect("请求体应为 JSON");
    assert_eq!(body["model"], json!("test-model"));
    assert_eq!(body["temperature"], json!(0));
    assert_eq!(body["max_tokens"], json!(64));
    assert_eq!(body["messages"][0]["role"], json!("system"));
    assert_eq!(body["messages"][1]["role"], json!("user"));
    assert!(body.get("stream").is_none());
    let user: Value = serde_json::from_str(
        body["messages"][1]["content"]
            .as_str()
            .expect("用户消息应为字符串"),
    )
    .expect("用户消息应为 JSON");
    assert_eq!(user["word"], json!("bank"));
    assert_eq!(user["locale"], json!("en"));
    assert_eq!(user["senses"][0]["id"], json!("e0-s1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_rejects_html_definition() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(llm_response("<b>the land alongside a river</b>"))
        .mount(&server)
        .await;

    let error = run_generate(
        llm_settings(endpoint(&server, "/v1/chat/completions"), 500),
        sample_senses(),
        None,
    )
    .await
    .expect_err("HTML 定义应被拒绝");
    assert_eq!(error.code(), "LLM_BAD_RESPONSE");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_rejects_non_json_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "content": "not json at all" } }],
        })))
        .mount(&server)
        .await;

    let error = run_generate(
        llm_settings(endpoint(&server, "/v1/chat/completions"), 500),
        sample_senses(),
        None,
    )
    .await
    .expect_err("坏 JSON 应被拒绝");
    assert_eq!(error.code(), "LLM_BAD_RESPONSE");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_timeout_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(llm_response("river land").set_delay(Duration::from_millis(800)))
        .mount(&server)
        .await;

    let error = run_generate(
        llm_settings(endpoint(&server, "/v1/chat/completions"), 200),
        sample_senses(),
        None,
    )
    .await
    .expect_err("应当超时");
    assert_eq!(error, LlmError::Timeout);
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_401_is_bad_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "error": "unauthorized" })))
        .mount(&server)
        .await;

    let error = run_generate(
        llm_settings(endpoint(&server, "/v1/chat/completions"), 500),
        sample_senses(),
        None,
    )
    .await
    .expect_err("401 应失败");
    assert_eq!(error.code(), "LLM_BAD_RESPONSE");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_blocks_remote_endpoint_without_sending() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(llm_response("river land"))
        .mount(&server)
        .await;
    let blocked = format!(
        "{}/v1/chat/completions",
        server.uri().replace("127.0.0.1", "0.0.0.0")
    );

    let error = run_generate(llm_settings(blocked, 500), sample_senses(), None)
        .await
        .expect_err("应被阻止");
    assert_eq!(error, LlmError::RemoteBlocked);
    let received = server
        .received_requests()
        .await
        .expect("wiremock 应记录请求");
    assert!(received.is_empty(), "被阻止的端点不应发出请求");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_reads_api_key_from_env() {
    let variable = format!("LEXIGRAB_TEST_LLM_KEY_{}", std::process::id());
    std::env::set_var(&variable, "secret-token");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(header("authorization", "Bearer secret-token"))
        .respond_with(llm_response("river land"))
        .mount(&server)
        .await;

    let settings = LlmSettings {
        api_key_env: variable.clone(),
        ..llm_settings(endpoint(&server, "/v1/chat/completions"), 500)
    };
    let result = run_generate(settings, sample_senses(), None).await;
    std::env::remove_var(&variable);
    assert!(result.is_ok(), "带 key 的请求应当成功: {result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn http_llm_omits_authorization_without_key() {
    let variable = format!("LEXIGRAB_TEST_MISSING_KEY_{}", std::process::id());
    std::env::remove_var(&variable);
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(llm_response("river land"))
        .mount(&server)
        .await;

    let settings = LlmSettings {
        api_key_env: variable,
        ..llm_settings(endpoint(&server, "/v1/chat/completions"), 500)
    };
    run_generate(settings, sample_senses(), None)
        .await
        .expect("无 key 时也应成功");

    let received = server
        .received_requests()
        .await
        .expect("wiremock 应记录请求");
    assert_eq!(received.len(), 1);
    assert!(received[0].headers.get("authorization").is_none());
}

// ---------------------------------------------------------------------------
// 状态机 fake
// ---------------------------------------------------------------------------

enum SelectorOutcome {
    Reply(SelectorReply),
    Error(SenseError),
}

struct FakeSelector {
    outcome: SelectorOutcome,
    calls: AtomicUsize,
}

impl FakeSelector {
    fn replying(selected_id: Option<&str>, confidence: Option<f32>) -> Self {
        Self {
            outcome: SelectorOutcome::Reply(SelectorReply {
                selected_id: selected_id.map(str::to_string),
                confidence,
                model: Some("fake-selector".to_string()),
                distribution: None,
            }),
            calls: AtomicUsize::new(0),
        }
    }

    fn failing(error: SenseError) -> Self {
        Self {
            outcome: SelectorOutcome::Error(error),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl SenseSelector for FakeSelector {
    fn select(
        &self,
        _settings: &SenseSettings,
        _word: &str,
        _sentence: &str,
        _senses: &[SenseCandidate],
    ) -> Result<SelectorReply, SenseError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match &self.outcome {
            SelectorOutcome::Reply(reply) => Ok(reply.clone()),
            SelectorOutcome::Error(error) => Err(error.clone()),
        }
    }
}

enum LlmOutcome {
    Definition(AiDefinition),
    Error(LlmError),
}

struct FakeLlm {
    outcome: LlmOutcome,
    calls: AtomicUsize,
    based_on: Mutex<Vec<Option<String>>>,
}

impl FakeLlm {
    fn defining() -> Self {
        Self {
            outcome: LlmOutcome::Definition(AiDefinition {
                definition_text: "the land alongside a river".to_string(),
                part_of_speech: Some("noun".to_string()),
                model: "fake-llm".to_string(),
                confidence: Some(0.7),
                based_on_sense_id: None,
                generated_at: 1_700_000_000,
            }),
            calls: AtomicUsize::new(0),
            based_on: Mutex::new(Vec::new()),
        }
    }

    fn failing(error: LlmError) -> Self {
        Self {
            outcome: LlmOutcome::Error(error),
            calls: AtomicUsize::new(0),
            based_on: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn based_on(&self) -> Vec<Option<String>> {
        self.based_on.lock().expect("based_on 锁").clone()
    }
}

impl LlmGenerator for FakeLlm {
    fn generate(
        &self,
        _settings: &LlmSettings,
        _word: &str,
        _sentence: &str,
        _senses: &[SenseCandidate],
        based_on_sense_id: Option<&str>,
    ) -> Result<AiDefinition, LlmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.based_on
            .lock()
            .expect("based_on 锁")
            .push(based_on_sense_id.map(str::to_string));
        match &self.outcome {
            LlmOutcome::Definition(definition) => {
                let mut definition = definition.clone();
                definition.based_on_sense_id = based_on_sense_id.map(str::to_string);
                Ok(definition)
            }
            LlmOutcome::Error(error) => Err(error.clone()),
        }
    }
}

struct FakeClock(Arc<AtomicI64>);

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

fn resolution_settings(llm_enabled: bool) -> Settings {
    Settings {
        sense: SenseSettings {
            enabled: true,
            ..SenseSettings::default()
        },
        llm: LlmSettings {
            enabled: llm_enabled,
            model: "fake-model".to_string(),
            ..LlmSettings::default()
        },
        ..Settings::default()
    }
}

fn sense_entries() -> Vec<Entry> {
    let mut entry = Entry::new(
        "bank",
        "<ol><li>(n.) financial institution</li><li>(n.) sloping land beside water</li></ol>",
    );
    entry.source_name = "Mini".to_string();
    vec![entry]
}

struct Scenario {
    settings: Settings,
    cache: CacheStore,
    entries: Vec<Entry>,
}

impl Scenario {
    fn new(llm_enabled: bool) -> Self {
        Self {
            settings: resolution_settings(llm_enabled),
            cache: CacheStore::in_memory().expect("打开内存缓存"),
            entries: sense_entries(),
        }
    }

    fn resolve(&self, selector: &dyn SenseSelector, llm: &dyn LlmGenerator) -> SenseResolution {
        self.resolve_with_query("q1", Some(SENTENCE), selector, llm)
    }

    fn resolve_with_query(
        &self,
        query_id: &str,
        sentence: Option<&str>,
        selector: &dyn SenseSelector,
        llm: &dyn LlmGenerator,
    ) -> SenseResolution {
        resolve_with(
            SenseInput {
                query_id,
                headword: "bank",
                sentence,
                entries: &self.entries,
                settings: &self.settings,
                cache: &self.cache,
            },
            selector,
            llm,
        )
    }
}

// ---------------------------------------------------------------------------
// 状态机：基础状态
// ---------------------------------------------------------------------------

#[test]
fn disabled_status_keeps_candidates_and_skips_network() {
    let mut scenario = Scenario::new(true);
    scenario.settings.sense.enabled = false;
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::Disabled);
    assert_eq!(resolution.candidates.len(), 2);
    assert_eq!(selector.calls(), 0);
    assert_eq!(llm.calls(), 0);
    assert_eq!(scenario.cache.stats().expect("统计").sense_entries, 0);
}

#[test]
fn blank_sentence_is_no_context_without_network() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    for sentence in [None, Some("   ")] {
        let resolution = scenario.resolve_with_query("q1", sentence, &selector, &llm);
        assert_eq!(resolution.status, SenseStatus::NoContext);
        assert_eq!(resolution.candidates.len(), 2);
    }
    assert_eq!(selector.calls(), 0);
    assert_eq!(llm.calls(), 0);
}

#[test]
fn high_confidence_selects_without_llm() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::Selected);
    assert_eq!(resolution.selected_sense_id.as_deref(), Some("e0-s2"));
    assert_eq!(resolution.confidence, Some(0.9));
    assert_eq!(resolution.selector_model.as_deref(), Some("fake-selector"));
    assert!(resolution.ai.is_none());
    assert!(resolution.error.is_none());
    assert_eq!(llm.calls(), 0);
}

#[test]
fn mid_confidence_is_ambiguous_without_llm() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.6));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::Ambiguous);
    assert_eq!(resolution.selected_sense_id.as_deref(), Some("e0-s1"));
    assert_eq!(resolution.confidence, Some(0.6));
    assert_eq!(llm.calls(), 0);
}

#[test]
fn thresholds_follow_configured_boundaries() {
    for (confidence, expected) in [
        (0.72, SenseStatus::Selected),
        (0.56, SenseStatus::Ambiguous),
        (0.55, SenseStatus::LowConfidence),
    ] {
        let scenario = Scenario::new(false);
        let selector = FakeSelector::replying(Some("e0-s1"), Some(confidence));
        let llm = FakeLlm::defining();
        let resolution = scenario.resolve(&selector, &llm);
        assert_eq!(
            resolution.status, expected,
            "conf={confidence} 应映射为 {expected:?}"
        );
    }
}

#[test]
fn low_confidence_triggers_llm_with_selected_sense() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.3));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert_eq!(resolution.selected_sense_id.as_deref(), Some("e0-s1"));
    let ai = resolution.ai.as_ref().expect("应有 AI 释义");
    assert_eq!(ai.definition_text, "the land alongside a river");
    assert_eq!(ai.model, "fake-llm");
    assert_eq!(llm.calls(), 1);
    assert_eq!(llm.based_on(), vec![Some("e0-s1".to_string())]);
}

#[test]
fn high_confidence_with_empty_selected_id_is_low_confidence() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(None, Some(0.95));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert_eq!(resolution.selected_sense_id, None);
    assert_eq!(llm.calls(), 1);
    assert_eq!(llm.based_on(), vec![None]);
}

#[test]
fn unknown_selected_id_is_low_confidence() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e9-s9"), Some(0.9));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert_eq!(resolution.selected_sense_id, None);
    assert!(resolution.candidates.len() == 2);
}

#[test]
fn selector_failure_is_unavailable_without_auto_llm() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::failing(SenseError::Timeout);
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::Unavailable);
    assert!(resolution
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("SENSE_SELECTOR_FAILED"));
    assert_eq!(resolution.candidates.len(), 2);
    assert_eq!(llm.calls(), 0);
    assert_eq!(scenario.cache.stats().expect("统计").sense_entries, 0);
}

#[test]
fn llm_failure_keeps_low_confidence() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.3));
    let llm = FakeLlm::failing(LlmError::Timeout);

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert!(resolution.ai.is_none());
    assert!(resolution
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("LLM_TIMEOUT"));
    assert_eq!(llm.calls(), 1);
}

#[test]
fn no_candidates_with_llm_enabled_returns_ai() {
    let mut scenario = Scenario::new(true);
    scenario.entries = vec![Entry::new("bank", "")];
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.9));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert!(resolution.candidates.is_empty());
    assert!(resolution.ai.is_some());
    assert_eq!(selector.calls(), 0);
    assert_eq!(llm.calls(), 1);
}

#[test]
fn no_candidates_without_llm_stays_low_confidence() {
    let mut scenario = Scenario::new(false);
    scenario.entries = vec![Entry::new("bank", "   ")];
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.9));
    let llm = FakeLlm::defining();

    let resolution = scenario.resolve(&selector, &llm);
    assert_eq!(resolution.status, SenseStatus::LowConfidence);
    assert!(resolution.ai.is_none());
    assert_eq!(selector.calls(), 0);
    assert_eq!(llm.calls(), 0);
}

// ---------------------------------------------------------------------------
// 状态机：缓存
// ---------------------------------------------------------------------------

#[test]
fn cache_hit_skips_selector_and_llm() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    let first = scenario.resolve(&selector, &llm);
    assert_eq!(first.status, SenseStatus::Selected);
    let second = scenario.resolve(&selector, &llm);
    assert_eq!(second.status, SenseStatus::Selected);
    assert_eq!(second.selected_sense_id.as_deref(), Some("e0-s2"));
    assert_eq!(selector.calls(), 1);
    assert_eq!(llm.calls(), 0);
}

#[test]
fn cache_hit_restores_ai_and_skips_llm() {
    let scenario = Scenario::new(true);
    let selector = FakeSelector::replying(Some("e0-s1"), Some(0.3));
    let llm = FakeLlm::defining();

    let first = scenario.resolve(&selector, &llm);
    assert!(first.ai.is_some());
    let second = scenario.resolve(&selector, &llm);
    assert_eq!(second.status, SenseStatus::LowConfidence);
    assert!(second.ai.is_some());
    assert_eq!(selector.calls(), 1);
    assert_eq!(llm.calls(), 1);
}

#[test]
fn cache_key_changes_when_model_changes() {
    let mut scenario = Scenario::new(false);
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    let first = scenario.resolve(&selector, &llm);
    assert_eq!(first.status, SenseStatus::Selected);
    assert_eq!(selector.calls(), 1);

    scenario.settings.llm.model = "other-model".to_string();
    let second = scenario.resolve(&selector, &llm);
    assert_eq!(second.status, SenseStatus::Selected);
    assert_eq!(selector.calls(), 2);
}

#[test]
fn cache_expires_after_ttl() {
    let mut scenario = Scenario::new(false);
    let now = Arc::new(AtomicI64::new(1_700_000_000));
    scenario.cache = CacheStore::in_memory()
        .expect("打开内存缓存")
        .with_clock(Arc::new(FakeClock(now.clone())));
    scenario.settings.llm.cache_ttl_hours = 1;
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    scenario.resolve(&selector, &llm);
    scenario.resolve(&selector, &llm);
    assert_eq!(selector.calls(), 1, "TTL 内应命中缓存");

    now.store(1_700_000_000 + 3_601, Ordering::SeqCst);
    scenario.resolve(&selector, &llm);
    assert_eq!(selector.calls(), 2, "过期后应重新请求");
}

#[test]
fn cached_resolution_uses_current_query_id() {
    let scenario = Scenario::new(false);
    let selector = FakeSelector::replying(Some("e0-s2"), Some(0.9));
    let llm = FakeLlm::defining();

    scenario.resolve_with_query("q1", Some(SENTENCE), &selector, &llm);
    let second = scenario.resolve_with_query("q2", Some(SENTENCE), &selector, &llm);
    assert_eq!(second.query_id, "q2");
    assert_eq!(second.headword, "bank");
    assert_eq!(selector.calls(), 1);
}

#[test]
fn failed_and_context_free_results_are_not_cached() {
    let unavailable = Scenario::new(true);
    unavailable.resolve(
        &FakeSelector::failing(SenseError::Failed("boom".to_string())),
        &FakeLlm::defining(),
    );
    assert_eq!(
        unavailable.cache.stats().expect("统计").sense_entries,
        0,
        "unavailable 不应写缓存"
    );

    let no_context = Scenario::new(true);
    no_context.resolve_with_query(
        "q1",
        None,
        &FakeSelector::replying(None, None),
        &FakeLlm::defining(),
    );
    assert_eq!(no_context.cache.stats().expect("统计").sense_entries, 0);

    let mut disabled = Scenario::new(true);
    disabled.settings.sense.enabled = false;
    disabled.resolve(
        &FakeSelector::replying(Some("e0-s1"), Some(0.9)),
        &FakeLlm::defining(),
    );
    assert_eq!(disabled.cache.stats().expect("统计").sense_entries, 0);
}

#[test]
fn status_serializes_as_snake_case() {
    let value = serde_json::to_value(SenseStatus::LowConfidence).expect("序列化状态");
    assert_eq!(value, json!("low_confidence"));
    let value = serde_json::to_value(SenseStatus::NoContext).expect("序列化状态");
    assert_eq!(value, json!("no_context"));
}
