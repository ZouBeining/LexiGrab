//! LLM 兜底释义生成。契约见 docs/BACKEND_STRUCTURE.md §7.6。
//!
//! 只在选择器低置信度或没有候选时由 `resolution` 调用；使用
//! OpenAI-compatible `POST {llm.endpoint}`，要求模型只返回 JSON。
//!
//! 隐私与安全：
//! - 默认只允许回环端点，非回环必须 `llm.allow_remote = true`；
//! - 被阻止的端点不会产生任何网络请求；
//! - API key 只从 `api_key_env` 指向的环境变量读取，不写配置、不写日志；
//! - 单词与原句只出现在请求体与系统提示词拼接的用户消息里，不写日志。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::{allowed_endpoint, http_client, AiDefinition, EndpointError, SenseCandidate, LOCALE};
use crate::core::config::LlmSettings;

/// 提示词版本；进入 `sense_cache` 的 key，改动提示词必须递增。
pub const PROMPT_VERSION: &str = "v1";

/// AI 定义的最大字符数。
pub const MAX_DEFINITION_CHARS: usize = 600;

/// 词典编纂者系统提示词：只允许一个 JSON 对象，禁止 Markdown/HTML。
const SYSTEM_PROMPT: &str = "You are a careful lexicographer. Given one word, the sentence it appears in, and a list of dictionary senses, write the definition that fits this sentence. Use the dictionary senses only as hints; decide from the sentence context. Reply with exactly one JSON object and nothing else: {\"definition\": string (plain text, no HTML, at most 600 characters), \"part_of_speech\": string or null, \"confidence\": number from 0 to 1, \"explanation\": short string}. Do not wrap the JSON in Markdown or code fences. Do not include any other keys.";

/// 可替换的 LLM 实现；只依赖配置与候选数据，便于用 fake 测状态机。
pub trait LlmGenerator: Send + Sync {
    fn generate(
        &self,
        settings: &LlmSettings,
        word: &str,
        sentence: &str,
        senses: &[SenseCandidate],
        based_on_sense_id: Option<&str>,
    ) -> Result<AiDefinition, LlmError>;
}

/// 通过 HTTP 调用 OpenAI-compatible 服务的实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct HttpLlm;

impl LlmGenerator for HttpLlm {
    fn generate(
        &self,
        settings: &LlmSettings,
        word: &str,
        sentence: &str,
        senses: &[SenseCandidate],
        based_on_sense_id: Option<&str>,
    ) -> Result<AiDefinition, LlmError> {
        if !settings.enabled {
            return Err(LlmError::Disabled);
        }
        let url = match allowed_endpoint(&settings.endpoint, settings.allow_remote) {
            Ok(url) => url,
            Err(EndpointError::RemoteBlocked) => return Err(LlmError::RemoteBlocked),
            Err(EndpointError::Invalid(message)) => return Err(LlmError::BadResponse(message)),
        };

        let senses_json: Vec<Value> = senses
            .iter()
            .map(|candidate| {
                json!({
                    "id": candidate.id,
                    "part_of_speech": candidate.part_of_speech,
                    "definition": candidate.definition_text,
                })
            })
            .collect();
        let user_content = serde_json::to_string(&json!({
            "word": word,
            "sentence": sentence,
            "senses": senses_json,
            "locale": LOCALE,
        }))
        .map_err(|error| LlmError::BadResponse(format!("组装 LLM 请求失败: {error}")))?;

        let timeout = Duration::from_millis(settings.timeout_ms);
        let client = http_client(timeout)
            .map_err(|error| LlmError::BadResponse(format!("创建 LLM 客户端失败: {error}")))?;

        let body = json!({
            "model": settings.model,
            "temperature": 0,
            "max_tokens": settings.max_tokens,
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": user_content },
            ],
        });
        let mut request = client.post(url).json(&body);
        if let Some(api_key) = api_key_from_env(&settings.api_key_env) {
            let header = reqwest::header::HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|_| LlmError::BadResponse("LLM API key 格式无效".to_string()))?;
            request = request.header(reqwest::header::AUTHORIZATION, header);
        }

        let response = request.send().map_err(map_request_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(LlmError::BadResponse(format!("LLM 返回 HTTP {status}")));
        }
        let payload: Value = response
            .json()
            .map_err(|error| LlmError::BadResponse(format!("LLM 响应不是合法 JSON: {error}")))?;
        let content = payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                LlmError::BadResponse("LLM 响应缺少 choices[0].message.content".to_string())
            })?;
        let content: Value = serde_json::from_str(content.trim())
            .map_err(|_| LlmError::BadResponse("LLM 返回的内容不是 JSON".to_string()))?;
        build_definition(&content, &settings.model, based_on_sense_id)
    }
}

/// 使用真实 HTTP 客户端的便捷入口。
pub fn generate(
    settings: &LlmSettings,
    word: &str,
    sentence: &str,
    senses: &[SenseCandidate],
    based_on_sense_id: Option<&str>,
) -> Result<AiDefinition, LlmError> {
    LlmGenerator::generate(
        &HttpLlm,
        settings,
        word,
        sentence,
        senses,
        based_on_sense_id,
    )
}

/// 从环境变量读取 API key；空串、未设置或变量名为空时不带 Authorization。
fn api_key_from_env(variable: &str) -> Option<String> {
    let variable = variable.trim();
    if variable.is_empty() {
        return None;
    }
    let value = std::env::var(variable).ok()?;
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// 传输层错误只区分超时与其他失败；错误文案不包含单词/原句/密钥。
fn map_request_error(error: reqwest::Error) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout
    } else {
        LlmError::BadResponse(format!("LLM 请求失败: {error}"))
    }
}

/// 校验并构造 `AiDefinition`：纯文本、≤600 字符、置信度越界置 None。
fn build_definition(
    payload: &Value,
    model: &str,
    based_on_sense_id: Option<&str>,
) -> Result<AiDefinition, LlmError> {
    let definition = payload
        .get("definition")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| LlmError::BadResponse("LLM 未返回 definition".to_string()))?;
    if contains_html_tag(definition) {
        return Err(LlmError::BadResponse("LLM 定义包含 HTML".to_string()));
    }

    Ok(AiDefinition {
        definition_text: definition.chars().take(MAX_DEFINITION_CHARS).collect(),
        part_of_speech: payload
            .get("part_of_speech")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        model: model.to_string(),
        confidence: payload
            .get("confidence")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .map(|value| value as f32),
        based_on_sense_id: based_on_sense_id.map(str::to_string),
        generated_at: now_unix(),
    })
}

/// 扫描形如 `<tag`、`</tag`、`<!` 的 HTML 起始标记。
fn contains_html_tag(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index] == b'<' {
            let next = bytes[index + 1];
            if next.is_ascii_alphabetic() || next == b'/' || next == b'!' {
                return true;
            }
        }
        index += 1;
    }
    false
}

/// 当前 unix 秒。
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// LLM 错误。错误码与 BACKEND_STRUCTURE §4.1 对齐。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    #[error("LLM 兜底未启用")]
    Disabled,
    #[error("LLM 地址不是回环地址，已阻止请求")]
    RemoteBlocked,
    #[error("LLM 响应超时")]
    Timeout,
    #[error("LLM 响应无效: {0}")]
    BadResponse(String),
}

impl LlmError {
    /// 固定错误码。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Disabled => "LLM_DISABLED",
            Self::RemoteBlocked => "LLM_REMOTE_BLOCKED",
            Self::Timeout => "LLM_TIMEOUT",
            Self::BadResponse(_) => "LLM_BAD_RESPONSE",
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

    #[test]
    fn disabled_llm_is_rejected_before_any_request() {
        let settings = LlmSettings::default();
        let error = generate(&settings, "bank", "a sentence", &[], None).expect_err("应返回错误");
        assert_eq!(error, LlmError::Disabled);
        assert_eq!(error.code(), "LLM_DISABLED");
    }

    #[test]
    fn remote_endpoint_is_blocked_without_allow_remote() {
        let settings = LlmSettings {
            enabled: true,
            endpoint: "http://192.0.2.10:11434/v1/chat/completions".to_string(),
            model: "local".to_string(),
            ..LlmSettings::default()
        };
        let error = generate(&settings, "bank", "a sentence", &[], None).expect_err("应返回错误");
        assert_eq!(error, LlmError::RemoteBlocked);
        assert_eq!(error.code(), "LLM_REMOTE_BLOCKED");
    }

    #[test]
    fn html_definitions_are_rejected() {
        let payload = json!({ "definition": "<b>river bank</b>" });
        let error = build_definition(&payload, "model", None).expect_err("应返回错误");
        assert!(matches!(error, LlmError::BadResponse(_)));
    }

    #[test]
    fn plain_angle_brackets_are_allowed() {
        let payload = json!({ "definition": "a < b and c > d", "confidence": 1.5 });
        let definition = build_definition(&payload, "model", Some("e0-s1")).expect("应成功");
        assert_eq!(definition.definition_text, "a < b and c > d");
        assert_eq!(definition.confidence, None);
        assert_eq!(definition.based_on_sense_id.as_deref(), Some("e0-s1"));
    }

    #[test]
    fn definitions_are_truncated_to_600_chars() {
        let long = "x".repeat(MAX_DEFINITION_CHARS + 40);
        let payload = json!({ "definition": long });
        let definition = build_definition(&payload, "model", None).expect("应成功");
        assert_eq!(
            definition.definition_text.chars().count(),
            MAX_DEFINITION_CHARS
        );
    }

    #[test]
    fn empty_definition_is_rejected() {
        let payload = json!({ "definition": "   " });
        let error = build_definition(&payload, "model", None).expect_err("应返回错误");
        assert!(matches!(error, LlmError::BadResponse(_)));
    }
}
