// SPDX-License-Identifier: AGPL-3.0-only
//! JEV-like 义项选择器的 HTTP 适配。契约见 docs/BACKEND_STRUCTURE.md §7.5。
//!
//! 只做一件事：把单词、原句与候选义项 POST 给用户自己包装的本地 HTTP 服务，
//! 返回模型自评的选中 id 与置信度。MVP 不启动任何本地进程，也不直接读 JEV CLI。
//!
//! 安全边界：
//! - 默认只允许回环端点，非回环必须 `selector_allow_remote = true`；
//! - 被阻止的端点不会产生任何网络请求；
//! - 只接受 HTTP 200 + JSON，不重试；
//! - 单词与原句不写入日志。

use std::time::Duration;

use serde_json::{json, Value};

use super::{allowed_endpoint, http_client, EndpointError, SenseCandidate, LOCALE};
use crate::core::config::SenseSettings;

/// 选择器统一返回的解析结果。
///
/// `selected_id` 可能是候选集合之外的 id；合法性由
/// [`crate::core::sense::resolution`] 的状态机判定，这里原样返回。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorReply {
    pub selected_id: Option<String>,
    pub confidence: Option<f32>,
    pub model: Option<String>,
    pub distribution: Option<serde_json::Value>,
}

/// 可替换的选择器实现；只依赖配置与候选数据，便于用 fake 测状态机。
pub trait SenseSelector: Send + Sync {
    fn select(
        &self,
        settings: &SenseSettings,
        word: &str,
        sentence: &str,
        senses: &[SenseCandidate],
    ) -> Result<SelectorReply, SenseError>;
}

/// 通过 HTTP 调用本地选择器的实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpSelector;

impl SenseSelector for HttpSelector {
    fn select(
        &self,
        settings: &SenseSettings,
        word: &str,
        sentence: &str,
        senses: &[SenseCandidate],
    ) -> Result<SelectorReply, SenseError> {
        if !settings.enabled {
            return Err(SenseError::Disabled);
        }
        let sentence = sentence.trim();
        if sentence.is_empty() {
            return Err(SenseError::NoContext);
        }

        let url =
            match allowed_endpoint(&settings.selector_endpoint, settings.selector_allow_remote) {
                Ok(url) => url,
                Err(EndpointError::RemoteBlocked) => return Err(SenseError::RemoteBlocked),
                Err(EndpointError::Invalid(message)) => return Err(SenseError::Failed(message)),
            };

        let timeout = Duration::from_millis(settings.selector_timeout_ms);
        let client = http_client(timeout)
            .map_err(|error| SenseError::Failed(format!("创建选择器客户端失败: {error}")))?;

        let body = json!({
            "word": word,
            "sentence": sentence,
            "locale": LOCALE,
            "senses": senses.iter().map(sense_payload).collect::<Vec<_>>(),
        });

        let response = client
            .post(url)
            .json(&body)
            .send()
            .map_err(map_request_error)?;
        let status = response.status();
        if status.as_u16() != 200 {
            return Err(SenseError::Failed(format!("选择器返回 HTTP {status}")));
        }
        let payload: Value = response
            .json()
            .map_err(|error| SenseError::Failed(format!("选择器响应不是合法 JSON: {error}")))?;
        Ok(parse_reply(&payload))
    }
}

/// 使用真实 HTTP 选择器的便捷入口。
pub fn select(
    settings: &SenseSettings,
    word: &str,
    sentence: &str,
    senses: &[SenseCandidate],
) -> Result<SelectorReply, SenseError> {
    SenseSelector::select(&HttpSelector, settings, word, sentence, senses)
}

/// 单个候选的请求字段：id、词性、纯文本定义。
fn sense_payload(candidate: &SenseCandidate) -> Value {
    json!({
        "id": candidate.id,
        "part_of_speech": candidate.part_of_speech,
        "definition": candidate.definition_text,
    })
}

/// 解析响应；confidence 缺失或不在 0–1 时按缺失处理，非法值不放大置信度。
fn parse_reply(payload: &Value) -> SelectorReply {
    SelectorReply {
        selected_id: payload
            .get("selected_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        confidence: payload
            .get("confidence")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .map(|value| value as f32),
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string),
        distribution: payload
            .get("distribution")
            .filter(|value| value.is_object() || value.is_array())
            .cloned(),
    }
}

/// 传输层错误只区分超时与其他失败；错误文案不包含单词/原句。
fn map_request_error(error: reqwest::Error) -> SenseError {
    if error.is_timeout() {
        SenseError::Timeout
    } else {
        SenseError::Failed(format!("选择器请求失败: {}", error.without_url()))
    }
}

/// 选择器错误。错误码与 BACKEND_STRUCTURE §4.1 对齐。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SenseError {
    #[error("语境辅助未启用")]
    Disabled,
    #[error("缺少原句")]
    NoContext,
    #[error("选择器地址不是回环地址，已阻止请求")]
    RemoteBlocked,
    #[error("语境模型响应超时")]
    Timeout,
    #[error("语境模型不可用: {0}")]
    Failed(String),
}

impl SenseError {
    /// 固定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "SENSE_DISABLED",
            Self::NoContext => "SENSE_NO_CONTEXT",
            Self::RemoteBlocked | Self::Timeout | Self::Failed(_) => "SENSE_SELECTOR_FAILED",
        }
    }

    /// 中文短句（失败时附可公开的诊断信息）。
    pub fn message(&self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(endpoint: &str) -> SenseSettings {
        SenseSettings {
            enabled: true,
            selector_endpoint: endpoint.to_string(),
            ..SenseSettings::default()
        }
    }

    #[test]
    fn disabled_selector_returns_disabled_before_network() {
        let settings = SenseSettings::default();
        let error = select(&settings, "bank", "a sentence", &[]).expect_err("应返回错误");
        assert_eq!(error, SenseError::Disabled);
        assert_eq!(error.code(), "SENSE_DISABLED");
    }

    #[test]
    fn blank_sentence_returns_no_context() {
        let error = select(&settings("http://127.0.0.1:1/select"), "bank", "  ", &[])
            .expect_err("应返回错误");
        assert_eq!(error, SenseError::NoContext);
        assert_eq!(error.code(), "SENSE_NO_CONTEXT");
    }

    #[test]
    fn non_loopback_endpoint_is_blocked_without_network() {
        let error = select(
            &settings("http://192.0.2.10:8787/v1/select"),
            "bank",
            "a sentence",
            &[],
        )
        .expect_err("应返回错误");
        assert_eq!(error, SenseError::RemoteBlocked);
        assert_eq!(error.code(), "SENSE_SELECTOR_FAILED");
    }

    #[test]
    fn unsupported_scheme_is_rejected() {
        let error = select(
            &settings("ftp://127.0.0.1/select"),
            "bank",
            "a sentence",
            &[],
        )
        .expect_err("应返回错误");
        assert!(
            matches!(error, SenseError::Failed(_)),
            "实际错误: {error:?}"
        );
    }

    #[test]
    fn reply_parsing_rejects_out_of_range_confidence() {
        let reply = parse_reply(&json!({
            "selected_id": "e0-s1",
            "confidence": 1.8,
            "distribution": { "e0-s1": 1.0 },
        }));
        assert_eq!(reply.selected_id.as_deref(), Some("e0-s1"));
        assert_eq!(reply.confidence, None);
        assert!(reply.distribution.is_some());
        assert_eq!(reply.model, None);
    }

    #[test]
    fn reply_parsing_drops_blank_selected_id() {
        let reply = parse_reply(&json!({ "selected_id": "  ", "confidence": 0.9 }));
        assert_eq!(reply.selected_id, None);
    }
}
