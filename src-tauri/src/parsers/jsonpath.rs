// SPDX-License-Identifier: AGPL-3.0-only
//! JSONPath 响应解析器。契约见 docs/BACKEND_STRUCTURE.md 7.2。
//!
//! 表达式命中对象数组（如 `$.entries[*]`）；命中数组会展开，非对象元素跳过。

use jsonpath_rust::JsonPath;
use serde_json::{Map, Value};

use crate::core::entry::Entry;
use crate::core::error::ParserError;

use super::response::{finalize_entries, ParseContext, RawEntry, ResponseParser};

/// 用 JSONPath 表达式抽取条目。
pub struct JsonPathParser {
    expression: String,
}

impl JsonPathParser {
    /// 校验表达式语法；空表达式视为无效。
    pub fn new(expression: impl Into<String>) -> Result<Self, ParserError> {
        let expression = expression.into().trim().to_string();
        if expression.is_empty() {
            return Err(ParserError::InvalidData("JSONPath 表达式为空".to_string()));
        }
        jsonpath_rust::parser::parse_json_path(&expression)
            .map_err(|error| ParserError::InvalidData(format!("JSONPath 表达式无效: {error}")))?;
        Ok(Self { expression })
    }

    pub fn expression(&self) -> &str {
        &self.expression
    }
}

impl ResponseParser for JsonPathParser {
    fn parse(&self, ctx: ParseContext<'_>) -> Result<Vec<Entry>, ParserError> {
        let value: Value = serde_json::from_str(ctx.response_body)
            .map_err(|error| ParserError::InvalidData(format!("响应不是合法 JSON: {error}")))?;
        let hits = value
            .query(&self.expression)
            .map_err(|error| ParserError::InvalidData(format!("JSONPath 查询失败: {error}")))?;

        let mut raw = Vec::new();
        for hit in hits {
            collect_entries(hit, &mut raw)?;
        }
        finalize_entries(raw, ctx.query)
    }
}

/// 展开数组；对象转成 `RawEntry`；标量跳过。
fn collect_entries(value: &Value, out: &mut Vec<RawEntry>) -> Result<(), ParserError> {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_entries(item, out)?;
            }
        }
        Value::Object(object) => {
            if let Some(entry) = raw_entry(object)? {
                out.push(entry);
            }
        }
        _ => {}
    }
    Ok(())
}

/// 转换单个对象；缺 `definition_html` 视为无条目（跳过）。
fn raw_entry(object: &Map<String, Value>) -> Result<Option<RawEntry>, ParserError> {
    let Some(definition_html) = optional_string(object, "definition_html")? else {
        return Ok(None);
    };
    let resources = match object.get("resources") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::String(url) => Ok(url.clone()),
                _ => Err(ParserError::InvalidData(
                    "resources 必须是字符串数组".to_string(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(ParserError::InvalidData(
                "resources 必须是字符串数组".to_string(),
            ))
        }
    };

    Ok(Some(RawEntry {
        headword: optional_string(object, "headword")?,
        definition_html,
        phonetic: optional_string(object, "phonetic")?,
        resources,
    }))
}

/// 读取可选字符串字段；缺失或 null 返回 `None`，其他类型视为结构错误。
fn optional_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, ParserError> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ParserError::InvalidData(format!("字段 {key} 必须是字符串"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(expression: &str, body: &str, query: &str) -> Result<Vec<Entry>, ParserError> {
        JsonPathParser::new(expression)?.parse(ParseContext {
            response_body: body,
            query,
        })
    }

    #[test]
    fn parses_standard_entries_array() {
        let body = r#"{"entries":[{"headword":"hello","definition_html":"<p>hi</p>","phonetic":"/h/","resources":["https://x/a.png"]}]}"#;
        let entries = parse("$.entries[*]", body, "hello").expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headword, "hello");
        assert_eq!(entries[0].phonetic.as_deref(), Some("/h/"));
        assert_eq!(entries[0].resources.len(), 1);
    }

    #[test]
    fn root_array_and_nested_path_work() {
        let body = r#"[{"definition_html":"<p>a</p>"},{"definition_html":"<p>b</p>"}]"#;
        assert_eq!(parse("$[*]", body, "q").expect("root").len(), 2);

        let nested = r#"{"data":{"items":[{"headword":"x","definition_html":"<p>a</p>"}]}}"#;
        assert_eq!(
            parse("$.data.items[*]", nested, "q").expect("nested").len(),
            1
        );
    }

    #[test]
    fn missing_definition_is_skipped_and_non_objects_ignored() {
        let body = r#"{"entries":[1,"x",null,{"headword":"a"},{"definition_html":"<p>ok</p>"}]}"#;
        let entries = parse("$.entries[*]", body, "q").expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headword, "q");
    }

    #[test]
    fn wrong_field_type_is_invalid() {
        let body = r#"{"entries":[{"definition_html":42}]}"#;
        let error = parse("$.entries[*]", body, "q").expect_err("type error");
        assert!(matches!(error, ParserError::InvalidData(_)));
    }

    #[test]
    fn bad_resource_url_is_dropped() {
        let body = r#"{"entries":[{"definition_html":"<p>a</p>","resources":["/rel.png","https://ok/x.png"]}]}"#;
        let entries = parse("$.entries[*]", body, "q").expect("parse");
        assert_eq!(entries[0].resources.len(), 1);
    }

    #[test]
    fn empty_result_is_not_an_error() {
        assert!(parse("$.entries[*]", r#"{"entries":[]}"#, "q")
            .expect("empty")
            .is_empty());
    }

    #[test]
    fn invalid_syntax_is_rejected() {
        assert!(JsonPathParser::new("$.[").is_err());
        assert!(JsonPathParser::new("   ").is_err());
    }

    #[test]
    fn broken_json_is_invalid_data() {
        let error = parse("$.entries[*]", "{not json", "q").expect_err("json");
        assert!(matches!(error, ParserError::InvalidData(_)));
    }

    #[test]
    fn result_is_capped_at_twenty() {
        let items: Vec<String> = (0..25)
            .map(|index| format!(r#"{{"definition_html":"<p>{index}</p>"}}"#))
            .collect();
        let body = format!(r#"{{"entries":[{}]}}"#, items.join(","));
        let entries = parse("$.entries[*]", &body, "q").expect("parse");
        assert_eq!(entries.len(), 20);
    }
}
