// SPDX-License-Identifier: AGPL-3.0-only
//! AnkiConnect 阻塞客户端。契约见 docs/BACKEND_STRUCTURE.md §6。
//!
//! 固定 POST `endpoint`，请求体形如
//! `{"action": "...", "version": 6, "params": {...}}`；响应
//! `{"result": ..., "error": ...}`。连接超时 1.5s、总超时 3s、不自动重试。
//! AnkiConnect 只监听本机回环地址，因此这里不做并发与重试设计。

use std::collections::BTreeMap;
use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::core::error::AnkiError;

/// AnkiConnect 协议版本，固定为 6。
const ANKI_CONNECT_VERSION: u32 = 6;
/// 建立 TCP 连接的超时。
const CONNECT_TIMEOUT: Duration = Duration::from_millis(1500);
/// 单次请求总超时（含连接与响应读取）。
const REQUEST_TIMEOUT: Duration = Duration::from_millis(3000);

/// addNote 请求体（已含 deck/model/fields/tags，不含 AnkiConnect 外层信封）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddNoteRequest {
    pub deck: String,
    pub model: String,
    pub fields: BTreeMap<String, String>,
    pub tags: Vec<String>,
}

impl AddNoteRequest {
    /// 发送前做本地形状校验，避免把明显无效的请求打到 Anki。
    fn validate(&self) -> Result<(), AnkiError> {
        if self.deck.trim().is_empty() {
            return Err(AnkiError::InvalidData("牌组名不能为空".to_string()));
        }
        if self.model.trim().is_empty() {
            return Err(AnkiError::InvalidData("模板名不能为空".to_string()));
        }
        if self.fields.is_empty() {
            return Err(AnkiError::InvalidData("note 字段不能为空".to_string()));
        }
        for name in self.fields.keys() {
            if name.trim().is_empty() {
                return Err(AnkiError::InvalidData("note 字段名不能为空".to_string()));
            }
        }
        for tag in &self.tags {
            if tag.trim().is_empty() {
                return Err(AnkiError::InvalidData("note 标签不能为空".to_string()));
            }
        }
        Ok(())
    }

    /// 按 §6 组装 `params.note`，始终带 `allowDuplicate: true`。
    fn to_note_value(&self) -> Value {
        let mut fields = Map::new();
        for (name, value) in &self.fields {
            fields.insert(name.clone(), Value::String(value.clone()));
        }
        json!({
            "deckName": self.deck,
            "modelName": self.model,
            "fields": fields,
            "tags": self.tags,
            "options": { "allowDuplicate": true },
        })
    }
}

/// 阻塞 AnkiConnect 客户端：连接超时 1.5s，总超时 3s，不重试。
pub struct AnkiClient {
    endpoint: String,
    client: reqwest::blocking::Client,
}

impl AnkiClient {
    pub fn new(endpoint: impl Into<String>) -> Result<Self, AnkiError> {
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(AnkiError::InvalidData(
                "AnkiConnect endpoint 不能为空".to_string(),
            ));
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| AnkiError::Api(format!("创建 AnkiConnect 客户端失败: {error}")))?;
        Ok(Self { endpoint, client })
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// `version`，返回 AnkiConnect 协议版本。
    pub fn version(&self) -> Result<u32, AnkiError> {
        let result = self.call("version", json!({}))?;
        let value = result
            .as_i64()
            .ok_or_else(|| AnkiError::Api("AnkiConnect version 返回值不是整数".to_string()))?;
        u32::try_from(value)
            .map_err(|_| AnkiError::Api(format!("AnkiConnect version 超出 u32 范围: {value}")))
    }

    pub fn deck_names(&self) -> Result<Vec<String>, AnkiError> {
        let result = self.call("deckNames", json!({}))?;
        string_list(result, "deckNames")
    }

    pub fn model_names(&self) -> Result<Vec<String>, AnkiError> {
        let result = self.call("modelNames", json!({}))?;
        string_list(result, "modelNames")
    }

    pub fn model_field_names(&self, model: &str) -> Result<Vec<String>, AnkiError> {
        if model.trim().is_empty() {
            return Err(AnkiError::InvalidData("模板名不能为空".to_string()));
        }
        let result = self.call("modelFieldNames", json!({ "modelName": model }))?;
        string_list(result, "modelFieldNames")
    }

    /// 创建笔记，返回 Anki 的 note id。始终允许重复，不暴露关闭开关。
    pub fn add_note(&self, request: &AddNoteRequest) -> Result<i64, AnkiError> {
        request.validate()?;
        let note = request.to_note_value();
        let result = self.call("addNote", json!({ "note": note }))?;
        result
            .as_i64()
            .ok_or_else(|| AnkiError::Api("AnkiConnect addNote 未返回卡片 id".to_string()))
    }

    /// 发送一次 AnkiConnect 调用并解包 `{result, error}` 信封。
    fn call(&self, action: &str, params: Value) -> Result<Value, AnkiError> {
        // 只记录动作名与端点，避免把整份 note 内容写进日志。
        tracing::debug!(action, endpoint = %self.endpoint, "发送 AnkiConnect 请求");
        let body = json!({
            "action": action,
            "version": ANKI_CONNECT_VERSION,
            "params": params,
        });
        let response = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .map_err(map_request_error)?;
        let status = response.status();
        if !status.is_success() {
            return Err(AnkiError::Api(format!("AnkiConnect 返回 HTTP {status}")));
        }
        let payload: Value = response
            .json()
            .map_err(|error| AnkiError::Api(format!("AnkiConnect 响应不是合法 JSON: {error}")))?;
        parse_envelope(payload)
    }
}

/// 把 reqwest 传输层错误映射为 `Unreachable`；结构与协议错误留给 `Api`。
fn map_request_error(error: reqwest::Error) -> AnkiError {
    if error.is_timeout()
        || error.is_connect()
        || error.is_dns()
        || error.is_request()
        || error.is_redirect()
    {
        AnkiError::Unreachable(format!("连接 AnkiConnect 失败: {error}"))
    } else {
        AnkiError::Api(format!("AnkiConnect 请求失败: {error}"))
    }
}

/// 解析 AnkiConnect 信封：`error` 为 null 时返回 `result`，否则映射错误。
fn parse_envelope(payload: Value) -> Result<Value, AnkiError> {
    let object = match payload {
        Value::Object(object) => object,
        _ => return Err(AnkiError::Api("AnkiConnect 响应不是 JSON 对象".to_string())),
    };
    let error = object
        .get("error")
        .ok_or_else(|| AnkiError::Api("AnkiConnect 响应缺少 error 字段".to_string()))?;
    let result = object.get("result").cloned();
    match error {
        Value::Null => {
            result.ok_or_else(|| AnkiError::Api("AnkiConnect 响应缺少 result 字段".to_string()))
        }
        // 兼容实现差异：error 为空白串视作没有错误。
        Value::String(message) if message.trim().is_empty() => {
            result.ok_or_else(|| AnkiError::Api("AnkiConnect 响应缺少 result 字段".to_string()))
        }
        Value::String(message) => Err(classify_api_error(message)),
        other => Err(AnkiError::Api(format!(
            "AnkiConnect 返回的 error 字段不是字符串: {other}"
        ))),
    }
}

/// 按错误文案分类：包含 duplicate（不区分大小写）即视为重复卡片。
fn classify_api_error(message: &str) -> AnkiError {
    if message.to_ascii_lowercase().contains("duplicate") {
        AnkiError::Duplicate
    } else {
        AnkiError::Api(message.to_string())
    }
}

/// 解析字符串数组结果，结构不符按 `Api` 错误处理。
fn string_list(value: Value, action: &str) -> Result<Vec<String>, AnkiError> {
    let items = value
        .as_array()
        .ok_or_else(|| AnkiError::Api(format!("AnkiConnect {action} 未返回数组")))?;
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let name = item
            .as_str()
            .ok_or_else(|| AnkiError::Api(format!("AnkiConnect {action} 返回了非字符串元素")))?;
        names.push(name.to_string());
    }
    Ok(names)
}
