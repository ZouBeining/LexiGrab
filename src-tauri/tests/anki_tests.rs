// SPDX-License-Identifier: AGPL-3.0-only
//! AnkiConnect 客户端与字段映射集成测试。
//!
//! reqwest 的阻塞客户端在构建与请求时都会同步等待内部运行时，因此客户端必须
//! 在 `tokio::task::spawn_blocking` 中创建与使用，不能出现在 async 上下文中。

use std::collections::BTreeMap;
use std::net::TcpListener;

use serde_json::{json, Value};
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use lexigrab_lib::anki::mapping::{
    build_fields, build_note_request, escape_html, html_with_breaks, validate_field_map,
};
use lexigrab_lib::anki::{AddNoteRequest, AnkiClient};
use lexigrab_lib::core::config::AnkiSettings;
use lexigrab_lib::core::entry::AnkiPayload;
use lexigrab_lib::core::error::AnkiError;

/// 在阻塞线程中创建客户端并执行一次同步调用。
async fn with_client<T, F>(endpoint: String, task: F) -> T
where
    T: Send + 'static,
    F: FnOnce(&AnkiClient) -> T + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let client = AnkiClient::new(endpoint).expect("创建客户端");
        task(&client)
    })
    .await
    .expect("spawn_blocking 任务应当完成")
}

fn expect_error<T>(result: Result<T, AnkiError>) -> AnkiError {
    match result {
        Ok(_) => panic!("预期返回错误，实际成功"),
        Err(error) => error,
    }
}

/// 启动一个只匹配 POST / 的 mock server，断言请求体完全等于期望值。
async fn mock_with_body(action: &str, params: Value, response: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(body_json(json!({
            "action": action,
            "version": 6,
            "params": params,
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .mount(&server)
        .await;
    server
}

/// 启动一个不校验请求体的 mock server，固定返回给定响应。
async fn mock_with_response(status: u16, response: Value) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(status).set_body_json(response))
        .mount(&server)
        .await;
    server
}

fn sample_request() -> AddNoteRequest {
    let mut fields = BTreeMap::new();
    fields.insert("Word".to_string(), "hello".to_string());
    fields.insert("Definition".to_string(), "<b>hi</b>".to_string());
    fields.insert("Sentence".to_string(), String::new());
    fields.insert("Source".to_string(), "Mini".to_string());
    AddNoteRequest {
        deck: "Default".to_string(),
        model: "Basic".to_string(),
        fields,
        tags: vec!["global-dict".to_string()],
    }
}

fn template_fields() -> Vec<String> {
    vec![
        "Word".to_string(),
        "Definition".to_string(),
        "Sentence".to_string(),
        "Source".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// AnkiConnect 客户端
// ---------------------------------------------------------------------------

#[test]
fn endpoint_is_preserved() {
    let client = AnkiClient::new("http://127.0.0.1:8765").expect("创建客户端");
    assert_eq!(client.endpoint(), "http://127.0.0.1:8765");
}

#[test]
fn empty_endpoint_is_rejected() {
    let error = expect_error(AnkiClient::new("   "));
    assert!(matches!(error, AnkiError::InvalidData(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn version_parses_integer_result() {
    let server = mock_with_body("version", json!({}), json!({"result": 6, "error": null})).await;
    let version = with_client(server.uri(), |client| client.version())
        .await
        .expect("version 应当成功");
    assert_eq!(version, 6);
}

#[tokio::test(flavor = "multi_thread")]
async fn deck_names_parse_string_array() {
    let server = mock_with_body(
        "deckNames",
        json!({}),
        json!({"result": ["Default", "LexiGrab"], "error": null}),
    )
    .await;

    let names = with_client(server.uri(), |client| client.deck_names())
        .await
        .expect("deck_names 应当成功");
    assert_eq!(names, vec!["Default".to_string(), "LexiGrab".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn model_names_parse_string_array() {
    let server = mock_with_body(
        "modelNames",
        json!({}),
        json!({"result": ["Basic", "Cloze"], "error": null}),
    )
    .await;

    let names = with_client(server.uri(), |client| client.model_names())
        .await
        .expect("model_names 应当成功");
    assert_eq!(names, vec!["Basic".to_string(), "Cloze".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn model_field_names_sends_model_name() {
    let server = mock_with_body(
        "modelFieldNames",
        json!({"modelName": "Basic"}),
        json!({"result": ["Word", "Definition", "Sentence", "Source"], "error": null}),
    )
    .await;

    let names = with_client(server.uri(), |client| client.model_field_names("Basic"))
        .await
        .expect("model_field_names 应当成功");
    assert_eq!(names, template_fields());
}

#[tokio::test(flavor = "multi_thread")]
async fn add_note_sends_contract_body_with_allow_duplicate() {
    let expected_note = json!({
        "deckName": "Default",
        "modelName": "Basic",
        "fields": {
            "Word": "hello",
            "Definition": "<b>hi</b>",
            "Sentence": "",
            "Source": "Mini",
        },
        "tags": ["global-dict"],
        "options": { "allowDuplicate": true },
    });
    let server = mock_with_body(
        "addNote",
        json!({ "note": expected_note }),
        json!({"result": 1234567890_i64, "error": null}),
    )
    .await;
    let request = sample_request();

    let note_id = with_client(server.uri(), move |client| client.add_note(&request))
        .await
        .expect("add_note 应当成功");
    assert_eq!(note_id, 1234567890);

    // 请求体匹配已经保证主体形状，这里再显式断言 allowDuplicate 恒为 true。
    let received = server
        .received_requests()
        .await
        .expect("wiremock 应记录请求");
    assert_eq!(received.len(), 1);
    let body: Value = serde_json::from_slice(&received[0].body).expect("请求体应为 JSON");
    assert_eq!(
        body["params"]["note"]["options"]["allowDuplicate"],
        json!(true)
    );
    assert_eq!(body["action"], json!("addNote"));
    assert_eq!(body["version"], json!(6));
}

#[tokio::test(flavor = "multi_thread")]
async fn add_note_duplicate_error_maps_to_duplicate() {
    // 错误文案大小写混合，验证分类不区分大小写。
    let server = mock_with_response(
        200,
        json!({
            "result": null,
            "error": "cannot create note because it is a Duplicate",
        }),
    )
    .await;
    let request = sample_request();

    let error =
        expect_error(with_client(server.uri(), move |client| client.add_note(&request)).await);
    assert_eq!(error, AnkiError::Duplicate);
    assert_eq!(error.code(), "ANKI_DUPLICATE");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_note_other_error_maps_to_api() {
    let server = mock_with_response(
        200,
        json!({"result": null, "error": "deck was not found: Default"}),
    )
    .await;
    let request = sample_request();

    let error =
        expect_error(with_client(server.uri(), move |client| client.add_note(&request)).await);
    assert_eq!(
        error,
        AnkiError::Api("deck was not found: Default".to_string())
    );
    assert_eq!(error.code(), "ANKI_API_ERROR");
}

#[tokio::test(flavor = "multi_thread")]
async fn non_success_http_status_maps_to_api() {
    let server = mock_with_response(500, json!({"message": "boom"})).await;

    let error = expect_error(with_client(server.uri(), |client| client.version()).await);
    assert!(matches!(error, AnkiError::Api(_)), "实际错误: {error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_json_body_maps_to_api() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let error = expect_error(with_client(server.uri(), |client| client.version()).await);
    assert!(matches!(error, AnkiError::Api(_)), "实际错误: {error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn non_integer_version_result_maps_to_api() {
    let server = mock_with_response(200, json!({"result": "6", "error": null})).await;

    let error = expect_error(with_client(server.uri(), |client| client.version()).await);
    assert!(matches!(error, AnkiError::Api(_)), "实际错误: {error:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn connection_refused_maps_to_unreachable() {
    // 先占用再释放端口，得到一个几乎肯定没有监听的本地端口。
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定临时端口");
    let port = listener.local_addr().expect("读取临时端口").port();
    drop(listener);

    let endpoint = format!("http://127.0.0.1:{port}");
    let error = expect_error(with_client(endpoint, |client| client.version()).await);
    assert!(
        matches!(error, AnkiError::Unreachable(_)),
        "实际错误: {error:?}"
    );
    assert_eq!(error.code(), "ANKI_UNREACHABLE");
}

#[tokio::test(flavor = "multi_thread")]
async fn add_note_rejects_invalid_requests_before_sending() {
    // 字段为空。
    let request = AddNoteRequest {
        deck: "Default".to_string(),
        model: "Basic".to_string(),
        fields: BTreeMap::new(),
        tags: Vec::new(),
    };
    let error = expect_error(
        with_client("http://127.0.0.1:1".to_string(), move |client| {
            client.add_note(&request)
        })
        .await,
    );
    assert!(matches!(error, AnkiError::InvalidData(_)));

    // 牌组为空。
    let mut request = sample_request();
    request.deck = "  ".to_string();
    let error = expect_error(
        with_client("http://127.0.0.1:1".to_string(), move |client| {
            client.add_note(&request)
        })
        .await,
    );
    assert!(matches!(error, AnkiError::InvalidData(_)));
    assert_eq!(error.code(), "INTERNAL");
}

#[tokio::test(flavor = "multi_thread")]
async fn model_field_names_rejects_empty_model() {
    let error = expect_error(
        with_client("http://127.0.0.1:1".to_string(), |client| {
            client.model_field_names("  ")
        })
        .await,
    );
    assert!(matches!(error, AnkiError::InvalidData(_)));
}

// ---------------------------------------------------------------------------
// 字段映射
// ---------------------------------------------------------------------------

#[test]
fn escape_html_escapes_all_special_characters() {
    assert_eq!(escape_html("&<>\"'"), "&amp;&lt;&gt;&quot;&#39;");
    assert_eq!(escape_html("纯文本"), "纯文本");
}

#[test]
fn html_with_breaks_converts_newlines() {
    assert_eq!(html_with_breaks("a\nb\r\nc\rd"), "a<br>b<br>c<br>d");
    assert_eq!(html_with_breaks("<x>\n&"), "&lt;x&gt;<br>&amp;");
}

#[test]
fn build_fields_escapes_text_and_keeps_definition_html() {
    let settings = AnkiSettings::default();
    let payload = AnkiPayload::new(
        "a<b & \"c\" 'd'",
        "<b>hi</b>",
        Some("line1\r\nline2\nline3".to_string()),
        "  ",
    );

    let fields = build_fields(&settings, &payload);
    assert_eq!(
        fields.get("Word").expect("Word 字段"),
        "a&lt;b &amp; &quot;c&quot; &#39;d&#39;"
    );
    assert_eq!(
        fields.get("Definition").expect("Definition 字段"),
        "<b>hi</b>"
    );
    assert_eq!(
        fields.get("Sentence").expect("Sentence 字段"),
        "line1<br>line2<br>line3"
    );
    // 空白来源替换为“未知来源”。
    assert_eq!(fields.get("Source").expect("Source 字段"), "未知来源");
}

#[test]
fn build_fields_handles_missing_and_empty_sentence() {
    let settings = AnkiSettings::default();
    let payload = AnkiPayload::new("hello", "<b>hi</b>", None, "Mini");
    let fields = build_fields(&settings, &payload);
    assert_eq!(fields.get("Sentence").expect("Sentence 字段"), "");

    let payload = AnkiPayload::new("hello", "<b>hi</b>", Some(String::new()), "Mini");
    let fields = build_fields(&settings, &payload);
    assert_eq!(fields.get("Sentence").expect("Sentence 字段"), "");
}

#[test]
fn validate_field_map_reports_first_missing_field() {
    let settings = AnkiSettings::default();
    let available = vec![
        "Word".to_string(),
        "Definition".to_string(),
        "Source".to_string(),
    ];
    let error = expect_error(validate_field_map(&settings, &available));
    assert_eq!(error, AnkiError::FieldMissing("Sentence".to_string()));
    assert_eq!(error.code(), "ANKI_FIELD_MISSING");
}

#[test]
fn validate_field_map_treats_empty_name_as_missing() {
    let mut settings = AnkiSettings::default();
    settings.field_map.sentence.clear();
    let error = expect_error(validate_field_map(&settings, &template_fields()));
    match error {
        AnkiError::FieldMissing(name) => assert!(name.is_empty(), "缺失字段名应为空: {name:?}"),
        other => panic!("预期 FieldMissing，实际: {other:?}"),
    }
}

#[test]
fn build_note_request_uses_settings_and_escaped_fields() {
    let settings = AnkiSettings::default();
    let payload = AnkiPayload::new("hello", "<b>hi</b>", Some("a & b".to_string()), "Mini");

    let request =
        build_note_request(&settings, &payload, &template_fields()).expect("组装请求应当成功");
    assert_eq!(request.deck, "Default");
    assert_eq!(request.model, "Basic");
    assert_eq!(request.tags, vec!["global-dict".to_string()]);
    assert_eq!(request.fields.len(), 4);
    assert_eq!(request.fields.get("Word").expect("Word"), "hello");
    assert_eq!(
        request.fields.get("Definition").expect("Definition"),
        "<b>hi</b>"
    );
    assert_eq!(
        request.fields.get("Sentence").expect("Sentence"),
        "a &amp; b"
    );
    assert_eq!(request.fields.get("Source").expect("Source"), "Mini");
}

#[test]
fn build_note_request_rejects_duplicate_field_mappings() {
    let mut settings = AnkiSettings::default();
    // word 与 definition 指向同一个模板字段。
    settings.field_map.definition = "Word".to_string();
    let payload = AnkiPayload::new("hello", "<b>hi</b>", None, "Mini");
    let available = vec![
        "Word".to_string(),
        "Sentence".to_string(),
        "Source".to_string(),
    ];

    let error = expect_error(build_note_request(&settings, &payload, &available));
    assert!(
        matches!(error, AnkiError::InvalidData(_)),
        "实际错误: {error:?}"
    );
}

#[test]
fn build_note_request_rejects_empty_word() {
    let settings = AnkiSettings::default();
    let payload = AnkiPayload::new("   ", "<b>hi</b>", None, "Mini");

    let error = expect_error(build_note_request(&settings, &payload, &template_fields()));
    assert!(
        matches!(error, AnkiError::InvalidData(_)),
        "实际错误: {error:?}"
    );
}

#[test]
fn build_note_request_rejects_oversized_definition() {
    let settings = AnkiSettings::default();
    let oversized = "x".repeat(lexigrab_lib::anki::MAX_DEFINITION_BYTES + 1);
    let payload = AnkiPayload::new("hello", oversized, None, "Mini");

    let error = expect_error(build_note_request(&settings, &payload, &template_fields()));
    assert!(
        matches!(error, AnkiError::InvalidData(_)),
        "实际错误: {error:?}"
    );
}
