//! 桥接 HTTP 契约测试：鉴权、Origin、CORS、请求体上限、限流、端点形状与错误映射。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §5。绝大多数用例走 `tower::ServiceExt::oneshot`，
//! 不启动真实端口；最后一个用例用临时端口验证只绑定回环地址。

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use axum::Router;
use lexigrab_lib::bridge::auth::RateLimiter;
use lexigrab_lib::bridge::dto::{
    AnkiResponse, HealthResponse, SelectionRequest, SelectionResponse, SenseReply,
};
use lexigrab_lib::bridge::error::BridgeError;
use lexigrab_lib::bridge::server::{router, router_with_limiter, BridgeBackend, BODY_LIMIT_BYTES};
use lexigrab_lib::core::entry::{AnkiPayload, Entry};
use serde_json::Value;
use tower::ServiceExt;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const WRONG_TOKEN: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

struct Fake {
    token: String,
    partial: bool,
    sense_available: bool,
    anki_unreachable: bool,
    sense_reply: SenseReply,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            token: TOKEN.to_string(),
            partial: false,
            sense_available: true,
            anki_unreachable: false,
            sense_reply: SenseReply::Pending,
        }
    }
}

impl BridgeBackend for Fake {
    fn token(&self) -> String {
        self.token.clone()
    }

    fn health(&self) -> HealthResponse {
        HealthResponse::current()
    }

    fn selection(&self, request: SelectionRequest) -> Result<SelectionResponse, BridgeError> {
        let mut entry = Entry::new(request.word.clone(), "<p>a definition</p>");
        entry.source_name = "Mini".to_string();
        Ok(SelectionResponse {
            query_id: "query-1".to_string(),
            entries: vec![entry],
            source_names: vec!["Mini".to_string()],
            partial: self.partial,
            sense_available: self.sense_available && request.has_sentence(),
            elapsed_ms: 12.5,
        })
    }

    fn sense(&self, query_id: &str) -> Result<SenseReply, BridgeError> {
        if query_id == "known" {
            Ok(self.sense_reply.clone())
        } else {
            Err(BridgeError::not_found("未知或已过期的查询"))
        }
    }

    fn anki(&self, _payload: AnkiPayload) -> Result<AnkiResponse, BridgeError> {
        if self.anki_unreachable {
            return Err(BridgeError::from_anki(
                lexigrab_lib::core::error::AnkiError::Unreachable("cli-down".to_string()),
            ));
        }
        Ok(AnkiResponse {
            note_id: 4321,
            exact_context_seen: true,
        })
    }
}

fn app() -> Router {
    router(Arc::new(Fake::default()))
}

fn app_with(fake: Fake) -> Router {
    router(Arc::new(fake))
}

fn request(method: Method, uri: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header("x-auth-token", token);
    }
    builder.body(Body::empty()).expect("request")
}

fn json_request(method: Method, uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-auth-token", TOKEN)
        .body(Body::from(body.to_string()))
        .expect("request")
}

async fn body_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("read body");
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

fn cors_header(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn missing_or_wrong_token_is_401_with_cors() {
    for token in [None, Some(WRONG_TOKEN), Some("")] {
        let response = app()
            .oneshot(request(Method::GET, "/v1/health", token))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
        let body = body_json(response).await;
        assert_eq!(body["code"], "AUTH_FAILED");
    }
}

#[tokio::test]
async fn token_grants_health_with_api_version() {
    let response = app()
        .oneshot(request(Method::GET, "/v1/health", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["ok"], true);
    assert_eq!(body["app"], "LexiGrab");
    assert_eq!(body["api_version"], 1);
}

#[tokio::test]
async fn extension_origin_is_accepted_and_echoed() {
    let mut request = request(Method::GET, "/v1/health", Some(TOKEN));
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("moz-extension://6f1c2a3b"),
    );
    let response = app().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(cors_header(&response), "moz-extension://6f1c2a3b");
}

#[tokio::test]
async fn foreign_origin_is_403_with_cors() {
    let mut request = request(Method::GET, "/v1/health", Some(TOKEN));
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("https://evil.example"),
    );
    let response = app().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    assert_eq!(body_json(response).await["code"], "ORIGIN_REJECTED");
}

#[tokio::test]
async fn preflight_allows_auth_header_without_token() {
    let mut request = request(Method::OPTIONS, "/v1/selection", None);
    request.headers_mut().insert(
        header::ORIGIN,
        HeaderValue::from_static("moz-extension://abc"),
    );
    let response = app().oneshot(request).await.expect("response");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let headers = response.headers();
    assert_eq!(cors_header(&response), "moz-extension://abc");
    let allow_headers = headers
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    assert!(allow_headers.contains("x-auth-token"));
    assert!(allow_headers.contains("content-type"));
    assert!(headers.contains_key(header::ACCESS_CONTROL_MAX_AGE));
}

#[tokio::test]
async fn selection_returns_contract_shape() {
    let response = app()
        .oneshot(json_request(
            Method::POST,
            "/v1/selection",
            r#"{"word":"hello","sentence":"Hello there.","url":"https://e.test/a","title":"T","domain":"e.test"}"#,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["query_id"], "query-1");
    assert_eq!(body["partial"], false);
    assert_eq!(body["sense_available"], true);
    assert_eq!(body["entries"][0]["headword"], "hello");
    assert_eq!(body["entries"][0]["source_name"], "Mini");
    assert_eq!(body["source_names"][0], "Mini");
    assert!(body["elapsed_ms"].is_number());
}

#[tokio::test]
async fn selection_without_sentence_disables_sense() {
    let response = app()
        .oneshot(json_request(
            Method::POST,
            "/v1/selection",
            r#"{"word":"hello"}"#,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["sense_available"], false);
}

#[tokio::test]
async fn selection_partial_result_is_flagged() {
    let response = app_with(Fake {
        partial: true,
        ..Fake::default()
    })
    .oneshot(json_request(
        Method::POST,
        "/v1/selection",
        r#"{"word":"hi"}"#,
    ))
    .await
    .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["partial"], true);
}

#[tokio::test]
async fn selection_without_word_is_400() {
    let response = app()
        .oneshot(json_request(
            Method::POST,
            "/v1/selection",
            r#"{"word":"   "}"#,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(body_json(response).await["code"], "INVALID_REQUEST");
}

#[tokio::test]
async fn oversized_body_is_413_with_cors() {
    let payload = format!("{{\"word\":\"{}\"}}", "a".repeat(BODY_LIMIT_BYTES + 64));
    let response = app()
        .oneshot(json_request(Method::POST, "/v1/selection", &payload))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

#[tokio::test]
async fn rate_limiter_rejects_burst_overflow_with_cors() {
    let app = router_with_limiter(
        Arc::new(Fake::default()),
        Arc::new(RateLimiter::new(0.0, 2.0)),
    );
    let first = app
        .clone()
        .oneshot(request(Method::GET, "/v1/health", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(first.status(), StatusCode::OK);
    let second = app
        .clone()
        .oneshot(request(Method::GET, "/v1/health", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(second.status(), StatusCode::OK);
    let third = app
        .oneshot(request(Method::GET, "/v1/health", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(third.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(third
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    assert_eq!(body_json(third).await["code"], "RATE_LIMITED");
}

#[tokio::test]
async fn sense_pending_is_202_and_unknown_is_404() {
    let response = app()
        .oneshot(request(Method::GET, "/v1/sense/known", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(body_json(response).await["status"], "pending");

    let response = app()
        .oneshot(request(Method::GET, "/v1/sense/unknown", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn sense_disabled_and_completed_are_200() {
    let response = app_with(Fake {
        sense_reply: SenseReply::Disabled,
        ..Fake::default()
    })
    .oneshot(request(Method::GET, "/v1/sense/known", Some(TOKEN)))
    .await
    .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["status"], "disabled");

    let response = app_with(Fake {
        sense_reply: SenseReply::Completed(serde_json::json!({
            "query_id": "known",
            "status": "selected",
            "selected_sense_id": "e0-s1"
        })),
        ..Fake::default()
    })
    .oneshot(request(Method::GET, "/v1/sense/known", Some(TOKEN)))
    .await
    .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await["selected_sense_id"], "e0-s1");
}

#[tokio::test]
async fn anki_success_and_unreachable_are_distinguishable() {
    let response = app()
        .oneshot(json_request(
            Method::POST,
            "/v1/anki",
            r#"{"word":"hello","definition_html":"<div>hi</div>","sentence":"Hello.","source":"Mini"}"#,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["note_id"], 4321);
    assert_eq!(body["exact_context_seen"], true);

    let response = app_with(Fake {
        anki_unreachable: true,
        ..Fake::default()
    })
    .oneshot(json_request(
        Method::POST,
        "/v1/anki",
        r#"{"word":"hello","definition_html":"<div>hi</div>"}"#,
    ))
    .await
    .expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body_json(response).await["code"], "ANKI_UNREACHABLE");
}

#[tokio::test]
async fn anki_without_definition_is_400() {
    let response = app()
        .oneshot(json_request(Method::POST, "/v1/anki", r#"{"word":"hi"}"#))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_route_is_404_with_cors() {
    let response = app()
        .oneshot(request(Method::GET, "/v1/nope", Some(TOKEN)))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(response
        .headers()
        .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
}

/// 真实端口：只绑定回环地址，并完成一次带令牌的 `/v1/health`。
#[tokio::test]
async fn real_port_serves_health_on_loopback() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind loopback");
    let address = listener.local_addr().expect("local addr");
    assert!(address.ip().is_loopback(), "bridge must bind loopback only");

    let server = axum::serve(listener, app());
    let server = tokio::spawn(async move {
        let _ = server.await;
    });

    let mut stream = tokio::net::TcpStream::connect(address)
        .await
        .expect("connect bridge");
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Auth-Token: {TOKEN}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .expect("read response");
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "unexpected: {response}"
    );
    assert!(response.contains("LexiGrab"));

    server.abort();
}
