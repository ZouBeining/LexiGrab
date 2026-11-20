// SPDX-License-Identifier: AGPL-3.0-only
//! 运行时事件到 Tauri 事件的桥。契约见 docs/BACKEND_STRUCTURE.md §4.2。
//!
//! 查询在 blocking 线程里执行，`AppHandle` 可跨线程 clone，因此事件 sink 能
//! 直接使用后台线程；远程失败只发提示事件，不产生模态错误。

use serde_json::json;
use tauri::{AppHandle, Emitter};

use crate::core::runtime::{EventSink, RuntimeEvent};

/// 把 [`RuntimeEvent`] 转成前端订阅的事件。
#[derive(Clone)]
pub struct TauriEventSink {
    app: AppHandle,
}

impl TauriEventSink {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit_payload(&self, event: &str, payload: serde_json::Value) {
        if let Err(error) = self.app.emit(event, payload) {
            tracing::warn!(error = %error, event, "发送运行时事件失败");
        }
    }
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::RemoteResult {
                query_id,
                source_id,
                source_name,
                entries,
                cached,
                elapsed_ms,
            } => self.emit_payload(
                "lookup://remote-result",
                json!({
                    "query_id": query_id,
                    "source_id": source_id,
                    "source_name": source_name,
                    "entries": entries,
                    "cached": cached,
                    "elapsed_ms": elapsed_ms,
                }),
            ),
            RuntimeEvent::RemoteError {
                query_id,
                source_id,
                code,
                message,
            } => self.emit_payload(
                "lookup://remote-error",
                json!({
                    "query_id": query_id,
                    "source_id": source_id,
                    "code": code,
                    "message": message,
                }),
            ),
            RuntimeEvent::SourceFailure {
                source_id, code, ..
            } => {
                // M2 起单源失败只记录日志，不弹错误；远程失败由上面的提示事件承担。
                tracing::warn!(code, source_id, "数据源查询失败，已隔离");
            }
            RuntimeEvent::SourceSlow {
                source_id,
                elapsed_ms,
                ..
            } => {
                tracing::debug!(source_id, elapsed_ms, "数据源查询较慢");
            }
        }
    }
}
