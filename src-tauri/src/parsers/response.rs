// SPDX-License-Identifier: AGPL-3.0-only
//! 远程响应解析器契约。契约见 docs/BACKEND_STRUCTURE.md 7.2 与 7.3。
//!
//! 解析器只把响应体映射为 `Entry`，不填 `source_name`、不读网络；数据源层负责
//! 填源名并按显示上限截断。

use serde::Deserialize;
use url::Url;

use crate::core::entry::{Entry, ResourceRef};
use crate::core::error::ParserError;
use crate::core::html::{sanitize, MAX_ENTRY_HTML_BYTES};

/// 每个响应最多保留的条目数，与路由层安全上限一致。
pub const MAX_ENTRIES: usize = 20;

/// 单次解析的输入；`response_body` 是已完整读取的响应体。
pub struct ParseContext<'a> {
    pub response_body: &'a str,
    pub query: &'a str,
}

/// 响应解析器契约，见 docs/BACKEND_STRUCTURE.md 7.2。
pub trait ResponseParser: Send + Sync {
    fn parse(&self, ctx: ParseContext<'_>) -> Result<Vec<Entry>, ParserError>;
}

/// 解析器产出的条目，字段尚未校验/清洗。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RawEntry {
    pub headword: Option<String>,
    pub definition_html: String,
    pub phonetic: Option<String>,
    pub resources: Vec<String>,
}

/// 统一校验并清洗解析结果：补词头、跳过空定义与超大 HTML、过滤非 http(s) 资源、截断。
pub fn finalize_entries(raw: Vec<RawEntry>, query: &str) -> Result<Vec<Entry>, ParserError> {
    let mut entries = Vec::new();
    for item in raw {
        if entries.len() >= MAX_ENTRIES {
            break;
        }
        if item.definition_html.trim().is_empty() {
            continue;
        }
        if item.definition_html.len() > MAX_ENTRY_HTML_BYTES {
            tracing::debug!(
                bytes = item.definition_html.len(),
                limit = MAX_ENTRY_HTML_BYTES,
                "跳过超大词条 HTML"
            );
            continue;
        }

        let headword = item
            .headword
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| query.to_string());
        let phonetic = item
            .phonetic
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let definition_html = sanitize(&item.definition_html)?;

        let mut resources: Vec<ResourceRef> = Vec::new();
        for raw_url in item.resources {
            let Some(url) = absolute_http_url(&raw_url) else {
                continue;
            };
            let resource = ResourceRef::Remote { url };
            if !resources.contains(&resource) {
                resources.push(resource);
            }
        }

        entries.push(Entry {
            headword,
            definition_html,
            phonetic,
            resources,
            source_name: String::new(),
        });
    }
    Ok(entries)
}

/// 只接受可解析的绝对 http/https URL；相对路径、其他 scheme 返回 `None`。
fn absolute_http_url(value: &str) -> Option<String> {
    let parsed = Url::parse(value.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    Some(parsed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(definition_html: &str) -> RawEntry {
        RawEntry {
            headword: None,
            definition_html: definition_html.to_string(),
            phonetic: None,
            resources: Vec::new(),
        }
    }

    #[test]
    fn missing_headword_falls_back_to_query() {
        let entries = finalize_entries(vec![raw("<b>hi</b>")], "hello").expect("finalize");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].headword, "hello");
        assert_eq!(entries[0].definition_html, "<b>hi</b>");
        assert!(entries[0].source_name.is_empty());
    }

    #[test]
    fn blank_definition_is_skipped() {
        let entries = finalize_entries(vec![raw("   "), raw("<p>x</p>")], "hello").expect("ok");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn resources_keep_only_absolute_http_urls() {
        let mut item = raw("<p>x</p>");
        item.resources = vec![
            "/img/a.png".to_string(),
            "ftp://example.com/a".to_string(),
            "https://example.com/a.png".to_string(),
            "not a url".to_string(),
        ];
        let entries = finalize_entries(vec![item], "hello").expect("ok");
        assert_eq!(
            entries[0].resources,
            vec![ResourceRef::Remote {
                url: "https://example.com/a.png".to_string()
            }]
        );
    }

    #[test]
    fn sanitize_strips_script_tags() {
        let entries =
            finalize_entries(vec![raw("<p>x</p><script>alert(1)</script>")], "hello").expect("ok");
        assert!(!entries[0].definition_html.contains("script"));
    }
}
