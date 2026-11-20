// SPDX-License-Identifier: AGPL-3.0-only
//! 桥接错误：HTTP 状态码、稳定错误码与 JSON 响应体。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §5.3。错误响应体固定为
//! `{ "code": "...", "message": "..." }`；CORS 头由外层守卫中间件补到所有响应，
//! 包括 401/403/413/429，扩展因此能读到错误正文。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::core::error::AnkiError;

/// 桥接错误码。成功响应不带这些值；扩展按状态码与错误码映射提示。
pub const CODE_AUTH_FAILED: &str = "AUTH_FAILED";
pub const CODE_ORIGIN_REJECTED: &str = "ORIGIN_REJECTED";
pub const CODE_RATE_LIMITED: &str = "RATE_LIMITED";
pub const CODE_PAYLOAD_TOO_LARGE: &str = "PAYLOAD_TOO_LARGE";
pub const CODE_INVALID_REQUEST: &str = "INVALID_REQUEST";
pub const CODE_TIMEOUT: &str = "TIMEOUT";
pub const CODE_NOT_FOUND: &str = "NOT_FOUND";
pub const CODE_INTERNAL: &str = "INTERNAL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    code: String,
    message: String,
}

impl BridgeError {
    pub fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

    /// 缺失或错误的 `X-Auth-Token`；不返回期望值。
    pub fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            CODE_AUTH_FAILED,
            "令牌缺失或错误，请在扩展设置中重新配对",
        )
    }

    /// 非 `moz-extension://` 的 Origin。
    pub fn forbidden_origin() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            CODE_ORIGIN_REJECTED,
            "只接受 Firefox 扩展发来的请求",
        )
    }

    pub fn rate_limited() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            CODE_RATE_LIMITED,
            "请求过于频繁，请稍后重试",
        )
    }

    pub fn payload_too_large(limit: usize) -> Self {
        Self::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            CODE_PAYLOAD_TOO_LARGE,
            format!("请求体超过上限（{} 字节）", limit),
        )
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, CODE_INVALID_REQUEST, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, CODE_NOT_FOUND, message)
    }

    pub fn timeout() -> Self {
        Self::new(StatusCode::GATEWAY_TIMEOUT, CODE_TIMEOUT, "桌面端处理超时")
    }

    pub fn internal(message: impl Into<String>) -> Self {
        let detail = message.into();
        tracing::warn!(detail = %detail, "桥接内部错误");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            CODE_INTERNAL,
            "桌面端内部错误，请查看日志",
        )
    }

    /// AnkiConnect 失败：不可达 503，字段缺失 422，其余 502。
    pub fn from_anki(error: AnkiError) -> Self {
        let status = match &error {
            AnkiError::Unreachable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AnkiError::FieldMissing(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AnkiError::Duplicate => StatusCode::CONFLICT,
            AnkiError::Api(_) | AnkiError::InvalidData(_) => StatusCode::BAD_GATEWAY,
        };
        Self::new(status, error.code(), error.to_string())
    }
}

impl std::fmt::Display for BridgeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        let body = ErrorBody {
            code: self.code,
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_and_origin_map_to_contract_statuses() {
        assert_eq!(BridgeError::unauthorized().status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            BridgeError::forbidden_origin().status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            BridgeError::rate_limited().status,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            BridgeError::payload_too_large(1024).status,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    fn anki_errors_keep_ipc_codes() {
        let unreachable = BridgeError::from_anki(AnkiError::Unreachable("down".into()));
        assert_eq!(unreachable.code, "ANKI_UNREACHABLE");
        assert_eq!(unreachable.status, StatusCode::SERVICE_UNAVAILABLE);
        let missing = BridgeError::from_anki(AnkiError::FieldMissing("Word".into()));
        assert_eq!(missing.status, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
