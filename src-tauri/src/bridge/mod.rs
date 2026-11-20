// SPDX-License-Identifier: AGPL-3.0-only
//! M3 本地 HTTP 桥接：只监听 `127.0.0.1`，供 Firefox 扩展查询与制卡。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §5，任务分解见 PLAN_M3 §4 T3.1。
//! 分层：`auth`（令牌/Origin/限流）、`dto`（请求响应与校验）、`error`（错误映射）、
//! `server`（路由与守卫中间件）、本文件（生命周期 + 用 AppState 实现后端）。
//!
//! 安全边界：绑定地址固定 `Ipv4Addr::LOCALHOST`，端口占用时状态为 failed，
//! 不静默换端口；令牌每次请求从配置读取，轮换后旧令牌立即 401。

pub mod auth;
pub mod dto;
pub mod error;
pub mod server;

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::bridge::dto::{
    AnkiResponse, HealthResponse, SelectionRequest, SelectionResponse, SenseReply,
};
use crate::bridge::error::BridgeError;
use crate::bridge::server::{BridgeBackend, COLLECT_ALL_DEADLINE};
use crate::core::config::BridgeSettings;
use crate::core::entry::AnkiPayload;
use crate::core::error::{IpcError, IpcResult};
use crate::core::state::{AppState, SenseQueryState};

pub use dto::API_VERSION;
pub use error::BridgeError as BridgeFailure;

/// 桥接运行状态，供设置页与 `bridge://status` 事件使用。
#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    /// 配置中是否启用。
    pub enabled: bool,
    /// 监听是否真的起来了。
    pub running: bool,
    /// 正在使用的端口（失败时是期望端口）。
    pub port: u16,
    /// `http://127.0.0.1:<port>`。
    pub url: String,
    pub api_version: u32,
    /// 最近一次成功请求的 unix 毫秒时间戳。
    pub last_success_ms: Option<i64>,
    /// 启动失败原因，例如端口占用。
    pub error: Option<String>,
}

/// 用 `AppState` 实现桥接后端；每次请求现读配置，令牌轮换无需重启。
pub struct AppBridge {
    app: AppHandle,
    last_success_ms: AtomicI64,
}

impl AppBridge {
    fn new(app: AppHandle) -> Self {
        Self {
            app,
            last_success_ms: AtomicI64::new(0),
        }
    }

    /// 最近成功时间；从未成功时为 None。
    pub fn last_success_ms(&self) -> Option<i64> {
        match self.last_success_ms.load(Ordering::Relaxed) {
            0 => None,
            value => Some(value),
        }
    }
}

impl BridgeBackend for AppBridge {
    fn token(&self) -> String {
        self.app.state::<AppState>().settings().bridge.token
    }

    fn health(&self) -> HealthResponse {
        HealthResponse::current()
    }

    fn selection(&self, request: SelectionRequest) -> Result<SelectionResponse, BridgeError> {
        let started = Instant::now();
        let state = self.app.state::<AppState>();
        let settings = state.settings();
        let collected = state.lookup_collect_all(&request.word, COLLECT_ALL_DEADLINE);

        // 本地媒体在浏览器里不可用：剥离成 [图片]/[音频] 占位，
        // 复用导出管线的清洗，避免把 mdx-res 链接注入页面。
        let entries: Vec<crate::core::entry::Entry> = collected
            .entries
            .into_iter()
            .map(|mut entry| {
                entry.definition_html =
                    crate::anki::export::strip_local_media(&entry.definition_html);
                entry.resources.clear();
                entry
            })
            .collect();

        let query_id = uuid::Uuid::new_v4().to_string();
        let sense_available = settings.sense.enabled && request.has_sentence();
        if sense_available {
            state.mark_sense_pending(&query_id);
            // 语境推荐不阻塞扩展首屏：后台解析，完成后写环形缓存并广播事件。
            let app = self.app.clone();
            let query_id_for_task = query_id.clone();
            let word = request.word.clone();
            let sentence = request.sentence.clone().unwrap_or_default();
            let sense_entries = entries.clone();
            std::thread::spawn(move || {
                let state = app.state::<AppState>();
                let resolution = state.run_sense_resolution(
                    &query_id_for_task,
                    &word,
                    Some(&sentence),
                    &sense_entries,
                );
                state.store_sense_resolution(&query_id_for_task, &resolution);
                if let Err(error) = app.emit("lookup://sense-result", &resolution) {
                    tracing::warn!(error = %error, "发送语境推荐事件失败");
                }
            });
        }

        Ok(SelectionResponse {
            query_id,
            entries,
            source_names: collected.source_names,
            partial: collected.partial,
            sense_available,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        })
    }

    fn sense(&self, query_id: &str) -> Result<SenseReply, BridgeError> {
        let state = self.app.state::<AppState>();
        if !state.settings().sense.enabled {
            return Ok(SenseReply::Disabled);
        }
        match state.sense_query_state(query_id) {
            Some(SenseQueryState::Pending) => Ok(SenseReply::Pending),
            Some(SenseQueryState::Resolved(value)) => Ok(SenseReply::Completed(value)),
            None => Err(BridgeError::not_found("未知或已过期的查询")),
        }
    }

    fn anki(&self, payload: AnkiPayload) -> Result<AnkiResponse, BridgeError> {
        let state = self.app.state::<AppState>();
        let settings = state.settings();
        let cards = state.cards();
        let deps = crate::anki::export::ExportDeps {
            settings: &settings.anki,
            cards: cards.as_ref(),
        };
        crate::anki::export::export_to_anki(deps, &payload)
            .map(|outcome| AnkiResponse {
                note_id: outcome.note_id,
                exact_context_seen: outcome.exact_context_seen,
            })
            .map_err(BridgeError::from_anki)
    }

    fn on_success(&self) {
        self.last_success_ms.store(now_unix_ms(), Ordering::Relaxed);
    }
}

/// 正在监听的桥接实例；drop 前由 `apply`/`stop` 发送关停信号。
struct Running {
    port: u16,
    shutdown: tokio::sync::oneshot::Sender<()>,
}

struct BridgeInner {
    enabled: bool,
    port: u16,
    running: Option<Running>,
    failure: Option<String>,
}

/// 桥接生命周期：随配置启动、停止与重启，并记录最近成功时间。
pub struct BridgeService {
    app: AppHandle,
    backend: Arc<AppBridge>,
    inner: Mutex<BridgeInner>,
}

impl BridgeService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            backend: Arc::new(AppBridge::new(app.clone())),
            app,
            inner: Mutex::new(BridgeInner {
                enabled: false,
                port: 0,
                running: None,
                failure: None,
            }),
        }
    }

    /// 按配置同步启停。返回端口占用等启动错误，不 panic、不换端口。
    pub fn apply(&self, settings: &BridgeSettings) -> Option<IpcError> {
        let failure = {
            let mut inner = lock(&self.inner);
            let unchanged = inner.enabled == settings.enabled
                && inner.port == settings.port
                && inner.running.is_some();
            if unchanged {
                return None;
            }
            if let Some(running) = inner.running.take() {
                tracing::info!(port = running.port, "停止桥接服务");
                let _ = running.shutdown.send(());
            }
            inner.enabled = settings.enabled;
            inner.port = settings.port;
            if !settings.enabled {
                inner.failure = None;
                None
            } else {
                match start_server(&self.backend, settings.port) {
                    Ok(running) => {
                        inner.running = Some(running);
                        inner.failure = None;
                        tracing::info!(port = settings.port, "桥接服务已启动");
                        None
                    }
                    Err(error) => {
                        tracing::warn!(code = %error.code, "桥接服务启动失败");
                        inner.failure = Some(error.message.clone());
                        Some(error)
                    }
                }
            }
        };
        self.emit_status();
        failure
    }

    /// 关闭监听（退出或测试收尾）。
    pub fn stop(&self) {
        let mut inner = lock(&self.inner);
        if let Some(running) = inner.running.take() {
            let _ = running.shutdown.send(());
        }
        inner.enabled = false;
    }

    pub fn status(&self) -> BridgeStatus {
        let inner = lock(&self.inner);
        let port = match &inner.running {
            Some(running) => running.port,
            None => inner.port,
        };
        BridgeStatus {
            enabled: inner.enabled,
            running: inner.running.is_some(),
            port,
            url: format!("http://127.0.0.1:{port}"),
            api_version: API_VERSION,
            last_success_ms: self.backend.last_success_ms(),
            error: inner.failure.clone(),
        }
    }

    fn emit_status(&self) {
        let status = self.status();
        if let Err(error) = self.app.emit("bridge://status", status) {
            tracing::warn!(error = %error, "发送桥接状态事件失败");
        }
    }
}

/// 绑定回环地址并在 Tauri 异步运行时里启动 axum。
fn start_server(backend: &Arc<AppBridge>, port: u16) -> Result<Running, IpcError> {
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(|error| {
        IpcError::new(
            "BRIDGE_PORT_IN_USE",
            format!("端口 {port} 已被占用或无法绑定: {error}"),
        )
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|error| IpcError::internal(format!("设置桥接监听器非阻塞失败: {error}")))?;

    let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();
    let router = server::router(Arc::clone(backend));
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                tracing::error!(error = %error, "桥接监听器交给 tokio 失败");
                return;
            }
        };
        let served = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = receiver.await;
            })
            .await;
        if let Err(error) = served {
            tracing::warn!(error = %error, "桥接服务异常退出");
        }
    });

    Ok(Running { port, shutdown })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// 最近成功时间窗口内是否算“已连接”（设置页文案用）。
pub fn recently_connected(status: &BridgeStatus, window: Duration) -> bool {
    match status.last_success_ms {
        Some(value) => {
            let age = now_unix_ms().saturating_sub(value);
            age >= 0 && (age as u128) <= window.as_millis()
        }
        None => false,
    }
}

/// 便捷包装：按当前配置重启桥接，忽略错误（错误已进状态）。
pub fn restart_from_settings(app: &AppHandle) -> IpcResult<BridgeStatus> {
    let service = app.state::<BridgeService>();
    let settings = app.state::<AppState>().settings();
    service.apply(&settings.bridge);
    Ok(service.status())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recently_connected_only_accepts_fresh_timestamps() {
        let mut status = BridgeStatus {
            enabled: true,
            running: true,
            port: 12787,
            url: "http://127.0.0.1:12787".to_string(),
            api_version: API_VERSION,
            last_success_ms: Some(now_unix_ms()),
            error: None,
        };
        assert!(recently_connected(&status, Duration::from_secs(60)));
        status.last_success_ms = Some(now_unix_ms() - 120_000);
        assert!(!recently_connected(&status, Duration::from_secs(60)));
        status.last_success_ms = None;
        assert!(!recently_connected(&status, Duration::from_secs(60)));
    }
}
