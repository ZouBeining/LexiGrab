//! Anki 字段映射。契约见 docs/BACKEND_STRUCTURE.md §2.4 与 §6。
//!
//! 只负责把 `AnkiPayload` 转换成 addNote 的字段形状；牌组、模板、标签均来自
//! `Settings.anki`，payload 不携带这些信息。

use std::collections::BTreeMap;

use crate::anki::client::AddNoteRequest;
use crate::core::config::AnkiSettings;
use crate::core::entry::AnkiPayload;
use crate::core::error::AnkiError;

/// `definition_html` 允许的最大字节数（512KB）。
pub const MAX_DEFINITION_BYTES: usize = 512 * 1024;

/// 来源为空时写入的占位文本。
pub const UNKNOWN_SOURCE: &str = "未知来源";

/// 把 payload 映射成 Anki 字段，word/sentence/source 做 HTML 转义与换行处理。
///
/// 该函数不返回错误：字段名与内容校验由 [`validate_field_map`] 与
/// [`build_note_request`] 负责；直接调用时调用方需自行保证字段名有效。
pub fn build_fields(settings: &AnkiSettings, payload: &AnkiPayload) -> BTreeMap<String, String> {
    let source = if payload.source.trim().is_empty() {
        UNKNOWN_SOURCE
    } else {
        payload.source.as_str()
    };
    let sentence = payload.sentence.as_deref().unwrap_or_default();
    let mut fields = BTreeMap::new();
    fields.insert(settings.field_map.word.clone(), escape_html(&payload.word));
    fields.insert(
        settings.field_map.definition.clone(),
        payload.definition_html.clone(),
    );
    fields.insert(
        settings.field_map.sentence.clone(),
        html_with_breaks(sentence),
    );
    fields.insert(settings.field_map.source.clone(), escape_html(source));
    fields
}

/// 导出前校验四个映射字段都存在于模板字段中；缺失返回第一个缺失字段名。
pub fn validate_field_map(settings: &AnkiSettings, available: &[String]) -> Result<(), AnkiError> {
    for name in mapped_field_names(settings) {
        // 空字段名等同于缺失，优先报出来便于用户定位配置问题。
        if name.trim().is_empty() || !available.iter().any(|candidate| candidate == name) {
            return Err(AnkiError::FieldMissing(name.to_string()));
        }
    }
    Ok(())
}

/// 组装 addNote 请求：先校验字段映射，再校验 payload 的硬性约束。
pub fn build_note_request(
    settings: &AnkiSettings,
    payload: &AnkiPayload,
    available: &[String],
) -> Result<AddNoteRequest, AnkiError> {
    validate_field_map(settings, available)?;
    ensure_unique_field_names(settings)?;
    if payload.word.trim().is_empty() {
        return Err(AnkiError::InvalidData("word 不能为空".to_string()));
    }
    if payload.definition_html.len() > MAX_DEFINITION_BYTES {
        return Err(AnkiError::InvalidData(format!(
            "definition_html 超过 {}KB 上限",
            MAX_DEFINITION_BYTES / 1024
        )));
    }
    Ok(AddNoteRequest {
        deck: settings.deck.clone(),
        model: settings.model.clone(),
        fields: build_fields(settings, payload),
        tags: settings.tags.clone(),
    })
}

/// HTML 转义 `& < > " '`，用于 word/sentence/source 等纯文本字段。
pub fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// 先做 HTML 转义，再把换行（含 CRLF/CR）换成 `<br>`。
pub fn html_with_breaks(text: &str) -> String {
    let escaped = escape_html(text);
    let mut result = String::with_capacity(escaped.len());
    let mut characters = escaped.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                result.push_str("<br>");
            }
            '\n' => result.push_str("<br>"),
            other => result.push(other),
        }
    }
    result
}

/// 四个映射字段名，顺序固定：word、definition、sentence、source。
fn mapped_field_names(settings: &AnkiSettings) -> [&str; 4] {
    [
        &settings.field_map.word,
        &settings.field_map.definition,
        &settings.field_map.sentence,
        &settings.field_map.source,
    ]
}

/// 两个映射指向同一字段会导致内容互相覆盖，按 InvalidData 拒绝。
fn ensure_unique_field_names(settings: &AnkiSettings) -> Result<(), AnkiError> {
    let names = mapped_field_names(settings);
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Err(AnkiError::InvalidData(format!("字段映射重复: {name}")));
        }
    }
    Ok(())
}
