// SPDX-License-Identifier: AGPL-3.0-only
//! 桥接 DTO 与请求校验。契约见 docs/BACKEND_STRUCTURE.md §5.2。
//!
//! 所有字段使用 snake_case。`SelectionRequest`/`AnkiRequest` 是外部输入，
//! 因此把校验放在这里，处理器只负责取状态、调后端与映射错误。

use serde::{Deserialize, Serialize};

use crate::bridge::error::BridgeError;
use crate::core::entry::{AnkiPayload, Entry};

/// 桥接 API 版本，`/v1/health` 返回给扩展做兼容判断。
pub const API_VERSION: u32 = 1;

/// 单个释义 HTML 上限，与 IPC `export_to_anki` 一致。
pub const MAX_DEFINITION_BYTES: usize = 512 * 1024;

/// 选中词最大字符数；扩展侧限制 64，这里放宽到 200 避免误伤。
const MAX_WORD_CHARS: usize = 200;
/// 原句最大字符数，超出截断而不是拒绝。
const MAX_SENTENCE_CHARS: usize = 2000;

/// `POST /v1/selection` 请求体。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SelectionRequest {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub sentence: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
}

impl SelectionRequest {
    /// 去掉首尾空白并校验必填字段；原句过长时截断。
    pub fn normalized(mut self) -> Result<Self, BridgeError> {
        self.word = self.word.trim().to_string();
        if self.word.is_empty() {
            return Err(BridgeError::invalid_request("word 不能为空"));
        }
        if self.word.chars().count() > MAX_WORD_CHARS {
            return Err(BridgeError::invalid_request(format!(
                "word 不能超过 {MAX_WORD_CHARS} 个字符"
            )));
        }
        self.sentence = truncate_chars(self.sentence, MAX_SENTENCE_CHARS);
        self.url = truncate_chars(self.url, 2048);
        self.title = truncate_chars(self.title, 512);
        self.domain = truncate_chars(self.domain, 253);
        Ok(self)
    }

    /// 是否具备语境辅助所需的最小上下文（有原句且不是空白）。
    pub fn has_sentence(&self) -> bool {
        self.sentence
            .as_deref()
            .is_some_and(|sentence| !sentence.trim().is_empty())
    }
}

/// `POST /v1/selection` 响应体。
#[derive(Debug, Clone, Serialize)]
pub struct SelectionResponse {
    pub query_id: String,
    pub entries: Vec<Entry>,
    pub source_names: Vec<String>,
    pub partial: bool,
    pub sense_available: bool,
    pub elapsed_ms: f64,
}

/// `POST /v1/anki` 请求体。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnkiRequest {
    #[serde(default)]
    pub word: String,
    #[serde(default)]
    pub definition_html: String,
    #[serde(default)]
    pub sentence: Option<String>,
    #[serde(default)]
    pub source: String,
}

impl AnkiRequest {
    /// 校验后转成内部 payload；服务端会再次清洗并剥离本地媒体。
    pub fn into_payload(mut self) -> Result<AnkiPayload, BridgeError> {
        self.word = self.word.trim().to_string();
        if self.word.is_empty() {
            return Err(BridgeError::invalid_request("word 不能为空"));
        }
        if self.word.chars().count() > MAX_WORD_CHARS {
            return Err(BridgeError::invalid_request(format!(
                "word 不能超过 {MAX_WORD_CHARS} 个字符"
            )));
        }
        if self.definition_html.len() > MAX_DEFINITION_BYTES {
            return Err(BridgeError::payload_too_large(MAX_DEFINITION_BYTES));
        }
        if self.definition_html.trim().is_empty() {
            return Err(BridgeError::invalid_request("definition_html 不能为空"));
        }
        Ok(AnkiPayload {
            word: self.word,
            definition_html: self.definition_html,
            sentence: truncate_chars(self.sentence, MAX_SENTENCE_CHARS),
            source: self.source.trim().to_string(),
            tags: Vec::new(),
        })
    }
}

/// `POST /v1/anki` 响应体；`exact_context_seen` 只提示同词同句同释义。
#[derive(Debug, Clone, Serialize)]
pub struct AnkiResponse {
    pub note_id: i64,
    pub exact_context_seen: bool,
}

/// `GET /v1/health` 响应体。
#[derive(Debug, Clone, Serialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub app: String,
    pub version: String,
    pub api_version: u32,
}

impl HealthResponse {
    pub fn current() -> Self {
        Self {
            ok: true,
            app: "LexiGrab".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            api_version: API_VERSION,
        }
    }
}

/// `/v1/sense/{query_id}` 的三种结果；M3 只会产生 Disabled/Pending。
#[derive(Debug, Clone)]
pub enum SenseReply {
    /// 语境辅助未启用：200 + `{ "status": "disabled" }`。
    Disabled,
    /// 仍在处理：202 + `{ "status": "pending" }`。
    Pending,
    /// 已完成：200 + `SenseResolution`（M4 写入）。
    Completed(serde_json::Value),
}

/// 处理中/未启用的状态体。
#[derive(Debug, Clone, Serialize)]
pub struct SenseStatusBody {
    pub status: &'static str,
}

impl SenseStatusBody {
    pub fn pending() -> Self {
        Self { status: "pending" }
    }

    pub fn disabled() -> Self {
        Self { status: "disabled" }
    }
}

/// 截断到指定字符数；空串归一为 `None`。
fn truncate_chars(value: Option<String>, max: usize) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::error::CODE_INVALID_REQUEST;

    #[test]
    fn selection_request_trims_and_requires_word() {
        let request = SelectionRequest {
            word: "  hello ".to_string(),
            ..Default::default()
        }
        .normalized()
        .expect("valid");
        assert_eq!(request.word, "hello");
        assert!(!request.has_sentence());

        let error = SelectionRequest::default()
            .normalized()
            .expect_err("empty word");
        assert_eq!(error.code, CODE_INVALID_REQUEST);
    }

    #[test]
    fn selection_truncates_long_sentence_and_drops_blank() {
        let long = "a".repeat(MAX_SENTENCE_CHARS + 50);
        let request = SelectionRequest {
            word: "x".to_string(),
            sentence: Some(long),
            title: Some("   ".to_string()),
            ..Default::default()
        }
        .normalized()
        .expect("valid");
        assert_eq!(
            request.sentence.as_deref().map(str::len),
            Some(MAX_SENTENCE_CHARS)
        );
        assert_eq!(request.title, None);
    }

    #[test]
    fn anki_request_rejects_oversize_html() {
        let error = AnkiRequest {
            word: "x".to_string(),
            definition_html: "a".repeat(MAX_DEFINITION_BYTES + 1),
            ..Default::default()
        }
        .into_payload()
        .expect_err("oversize");
        assert_eq!(error.status, axum::http::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn health_reports_api_version() {
        let health = HealthResponse::current();
        assert!(health.ok);
        assert_eq!(health.api_version, API_VERSION);
    }
}
