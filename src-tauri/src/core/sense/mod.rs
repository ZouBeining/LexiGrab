//! 语境辅助：JEV-like 义项选择器与 LLM 兜底。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §2.6、§7.4–§7.7：
//!
//! - 选择器与 LLM 默认只允许回环地址，远程端点必须显式开启；
//! - 单词与原句只用于请求体，不写入日志；
//! - 结果只作为建议，不修改 `Entry[]`，也不静默替换词典义项。

pub mod llm;
pub mod resolution;
pub mod selector;

use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::{Host, Url};

pub use crate::parsers::sense::SenseCandidate;
pub use llm::{generate, HttpLlm, LlmError, LlmGenerator};
pub use resolution::{resolve, resolve_with, SenseInput};
pub use selector::{select, HttpSelector, SelectorReply, SenseError, SenseSelector};

/// 建立 TCP 连接的超时；选择器与 LLM 共用。
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);

/// MVP 固定的语境语言。
pub(crate) const LOCALE: &str = "en";

/// 端点校验失败的原因。校验失败时不会发起任何网络请求。
pub(crate) enum EndpointError {
    /// 端点为空、scheme 非 http/https 或缺少主机名。
    Invalid(String),
    /// 非回环地址且未显式允许远程访问。
    RemoteBlocked,
}

/// 校验 http/https 端点；默认只允许回环主机。
pub(crate) fn allowed_endpoint(endpoint: &str, allow_remote: bool) -> Result<Url, EndpointError> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err(EndpointError::Invalid("端点不能为空".to_string()));
    }
    let url = Url::parse(trimmed)
        .map_err(|error| EndpointError::Invalid(format!("端点不是合法 URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(EndpointError::Invalid("端点只允许 http/https".to_string()));
    }
    if url.host_str().is_none() {
        return Err(EndpointError::Invalid("端点缺少主机名".to_string()));
    }
    if !allow_remote && !is_loopback(&url) {
        return Err(EndpointError::RemoteBlocked);
    }
    Ok(url)
}

/// 判断端点主机是否回环：`localhost`、127.0.0.0/8、`[::1]`。
fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

/// 构建阻塞 HTTP 客户端：连接超时固定 1.5s，总超时由调用方给出。
pub(crate) fn http_client(timeout: Duration) -> Result<reqwest::blocking::Client, reqwest::Error> {
    reqwest::blocking::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .build()
}

/// 语境辅助的整体状态。契约见 docs/BACKEND_STRUCTURE.md §2.6、§7.7。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenseStatus {
    Disabled,
    NoContext,
    Selected,
    Ambiguous,
    LowConfidence,
    Unavailable,
    Failed,
}

impl SenseStatus {
    /// 只有成功结果写入 `sense_cache`；失败与未启用状态一律不缓存。
    pub fn is_cacheable(&self) -> bool {
        matches!(self, Self::Selected | Self::Ambiguous | Self::LowConfidence)
    }
}

/// AI 生成的释义候选。纯文本，必须标注模型，用户确认后才可写卡。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiDefinition {
    pub definition_text: String,
    pub part_of_speech: Option<String>,
    pub model: String,
    pub confidence: Option<f32>,
    pub based_on_sense_id: Option<String>,
    pub generated_at: i64,
}

/// 一次语境辅助的完整结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenseResolution {
    pub query_id: String,
    pub headword: String,
    pub status: SenseStatus,
    pub selected_sense_id: Option<String>,
    pub confidence: Option<f32>,
    pub selector_model: Option<String>,
    pub ai: Option<AiDefinition>,
    pub error: Option<String>,
    pub candidates: Vec<SenseCandidate>,
}
