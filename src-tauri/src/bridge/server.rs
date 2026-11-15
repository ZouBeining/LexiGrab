//! 回环 HTTP 服务：路由、守卫中间件与阻塞边界。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §5。所有路由共用一个守卫中间件：
//! 预检不参与限流 → Origin 前缀 → 令牌桶 → `X-Auth-Token` 常量时间比较，
//! 最后给所有响应（含 401/403/413/429）补 CORS 头，扩展才能读到错误正文。
//!
//! 处理器保持薄：后端 trait 方法是同步阻塞的，统一在 `spawn_blocking` 中执行，
//! 并套 10s 处理超时，避免拖住异步运行时。

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::bridge::auth::{self, RateLimiter};
use crate::bridge::dto::{
    AnkiRequest, AnkiResponse, HealthResponse, SelectionRequest, SelectionResponse, SenseReply,
    SenseStatusBody,
};
use crate::bridge::error::BridgeError;
use crate::core::entry::AnkiPayload;

/// 请求体上限：1MB。
pub const BODY_LIMIT_BYTES: usize = 1024 * 1024;
/// 单次请求处理上限：10s。
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
/// `lookup_collect_all` 截止时间：3.5s。
pub const COLLECT_ALL_DEADLINE: Duration = Duration::from_millis(3500);

/// 鉴权头名称，小写便于 HeaderMap 查找。
const AUTH_HEADER: &str = "x-auth-token";

/// 桥接后端。实现方负责取配置、查词典、导出卡片；不处理 HTTP。
pub trait BridgeBackend: Send + Sync + 'static {
    /// 当前令牌（令牌轮换后无需重启服务）。
    fn token(&self) -> String;

    fn health(&self) -> HealthResponse;

    /// `lookup_collect_all` 的本地版本，截止时间由处理器传入。
    fn selection(&self, request: SelectionRequest) -> Result<SelectionResponse, BridgeError>;

    fn sense(&self, query_id: &str) -> Result<SenseReply, BridgeError>;

    /// 已校验的制卡负载；实现方负责清洗、导出与同语境提示。
    fn anki(&self, payload: AnkiPayload) -> Result<AnkiResponse, BridgeError>;

    /// 成功响应后的钩子，用于记录“最近成功请求时间”。
    fn on_success(&self) {}
}

/// 构造带默认限流器的路由。
pub fn router<B: BridgeBackend>(backend: Arc<B>) -> Router {
    router_with_limiter(backend, Arc::new(RateLimiter::default_limits()))
}

/// 可注入限流器的路由，测试用它固定突发容量。
pub fn router_with_limiter<B: BridgeBackend>(backend: Arc<B>, limiter: Arc<RateLimiter>) -> Router {
    let guard_state = GuardState {
        backend: Arc::clone(&backend),
        limiter,
    };
    Router::new()
        .route("/v1/health", get(health::<B>))
        .route("/v1/selection", post(selection::<B>))
        .route("/v1/sense/{query_id}", get(sense::<B>))
        .route("/v1/anki", post(anki::<B>))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .layer(middleware::from_fn_with_state(guard_state, guard::<B>))
        .with_state(backend)
}

/// 守卫中间件状态；手写 Clone 避免给 `B` 增加 `Clone` 约束。
struct GuardState<B> {
    backend: Arc<B>,
    limiter: Arc<RateLimiter>,
}

impl<B> Clone for GuardState<B> {
    fn clone(&self) -> Self {
        Self {
            backend: Arc::clone(&self.backend),
            limiter: Arc::clone(&self.limiter),
        }
    }
}

async fn guard<B: BridgeBackend>(
    State(state): State<GuardState<B>>,
    request: Request,
    next: Next,
) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let is_preflight = request.method() == Method::OPTIONS;

    let mut response = if is_preflight {
        // 预检不参与限流，也不需要令牌：浏览器不会附带自定义头。
        StatusCode::NO_CONTENT.into_response()
    } else if !auth::origin_allowed(origin.as_deref()) {
        BridgeError::forbidden_origin().into_response()
    } else if !state.limiter.check() {
        BridgeError::rate_limited().into_response()
    } else if !token_matches(state.backend.as_ref(), &request) {
        BridgeError::unauthorized().into_response()
    } else {
        let response = next.run(request).await;
        if response.status().is_success() {
            state.backend.on_success();
        }
        response
    };

    add_cors_headers(&mut response, origin.as_deref());
    response
}

/// 校验 `X-Auth-Token`；缺失时按空串参与常量时间比较。
fn token_matches<B: BridgeBackend>(backend: &B, request: &Request) -> bool {
    let provided = request
        .headers()
        .get(AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    auth::tokens_match(&backend.token(), provided)
}

/// 给所有响应补 CORS 头并回显 Origin，缺省用 `*` 供调试工具读取。
fn add_cors_headers(response: &mut Response, origin: Option<&str>) {
    let headers = response.headers_mut();
    let allow_origin = origin
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or_else(|| HeaderValue::from_static("*"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, allow_origin);
    headers.insert(
        HeaderName::from_static("vary"),
        HeaderValue::from_static("Origin"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-auth-token"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("600"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
}

async fn health<B: BridgeBackend>(State(backend): State<Arc<B>>) -> Json<HealthResponse> {
    Json(backend.health())
}

async fn selection<B: BridgeBackend>(
    State(backend): State<Arc<B>>,
    Json(request): Json<SelectionRequest>,
) -> Result<Json<SelectionResponse>, BridgeError> {
    let request = request.normalized()?;
    run_blocking(backend, move |backend| backend.selection(request))
        .await
        .map(Json)
}

async fn sense<B: BridgeBackend>(
    State(backend): State<Arc<B>>,
    Path(query_id): Path<String>,
) -> Result<Response, BridgeError> {
    let reply = run_blocking(backend, move |backend| backend.sense(&query_id)).await?;
    Ok(match reply {
        SenseReply::Disabled => (StatusCode::OK, Json(SenseStatusBody::disabled())).into_response(),
        SenseReply::Pending => {
            (StatusCode::ACCEPTED, Json(SenseStatusBody::pending())).into_response()
        }
        SenseReply::Completed(value) => (StatusCode::OK, Json(value)).into_response(),
    })
}

async fn anki<B: BridgeBackend>(
    State(backend): State<Arc<B>>,
    Json(request): Json<AnkiRequest>,
) -> Result<Json<AnkiResponse>, BridgeError> {
    // 校验放在阻塞派发之前，避免把明显非法的请求送进 Anki 管线。
    let payload = request.into_payload()?;
    run_blocking(backend, move |backend| backend.anki(payload.clone()))
        .await
        .map(Json)
}

async fn not_found() -> BridgeError {
    BridgeError::not_found("未知的桥接端点")
}

/// 在阻塞线程执行后端调用，并套 10s 超时。
async fn run_blocking<B, T, F>(backend: Arc<B>, work: F) -> Result<T, BridgeError>
where
    B: BridgeBackend + 'static,
    T: Send + 'static,
    F: FnOnce(&B) -> Result<T, BridgeError> + Send + 'static,
{
    let task = tokio::task::spawn_blocking(move || work(&backend));
    match tokio::time::timeout(PROCESS_TIMEOUT, task).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(BridgeError::internal(format!("阻塞任务异常: {error}"))),
        Err(_) => Err(BridgeError::timeout()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::auth::RateLimiter;
    use axum::body::Body;
    use tower::ServiceExt;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    struct Fake;

    impl BridgeBackend for Fake {
        fn token(&self) -> String {
            TOKEN.to_string()
        }

        fn health(&self) -> HealthResponse {
            HealthResponse::current()
        }

        fn selection(&self, request: SelectionRequest) -> Result<SelectionResponse, BridgeError> {
            Ok(SelectionResponse {
                query_id: "q1".to_string(),
                entries: Vec::new(),
                source_names: Vec::new(),
                partial: false,
                sense_available: request.has_sentence(),
                elapsed_ms: 1.0,
            })
        }

        fn sense(&self, query_id: &str) -> Result<SenseReply, BridgeError> {
            match query_id {
                "known" => Ok(SenseReply::Pending),
                _ => Err(BridgeError::not_found("未知查询")),
            }
        }

        fn anki(&self, _payload: AnkiPayload) -> Result<AnkiResponse, BridgeError> {
            Ok(AnkiResponse {
                note_id: 7,
                exact_context_seen: false,
            })
        }
    }

    fn app() -> Router {
        router(Arc::new(Fake))
    }

    fn request(method: Method, uri: &str, token: Option<&str>) -> Request {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            builder = builder.header(AUTH_HEADER, token);
        }
        builder.body(Body::empty()).expect("request")
    }

    #[tokio::test]
    async fn health_requires_token_and_echoes_cors() {
        let response = app()
            .oneshot(request(Method::GET, "/v1/health", None))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));

        let response = app()
            .oneshot(request(Method::GET, "/v1/health", Some(TOKEN)))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn non_extension_origin_is_forbidden() {
        let mut request = request(Method::GET, "/v1/health", Some(TOKEN));
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("https://example.com"),
        );
        let response = app().oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn preflight_is_not_rate_limited() {
        let limiter = Arc::new(RateLimiter::new(0.0, 0.0));
        let app = router_with_limiter(Arc::new(Fake), limiter);
        let mut request = request(Method::OPTIONS, "/v1/selection", None);
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_static("moz-extension://abc"),
        );
        let response = app.oneshot(request).await.expect("response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("moz-extension://abc"))
        );
    }
}
