// SPDX-License-Identifier: AGPL-3.0-only
//! MDX HTML 清洗与资源改写。契约见 docs/BACKEND_STRUCTURE.md 2.1 与 7.1。
//!
//! 处理顺序：先用 lol_html 改写资源路径与链接，再用 ammonia 白名单清洗。
//! 本地资源统一改写为 `mdx-res://{dict_id}/{percent-encoded path}`。

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use lol_html::html_content::Element;
use lol_html::{rewrite_str, ElementContentHandlers, HandlerResult, RewriteStrSettings, Selector};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::core::entry::ResourceRef;
use crate::core::error::ParserError;

/// 单条 HTML 上限，超出由上层跳过。
pub const MAX_ENTRY_HTML_BYTES: usize = 512 * 1024;

/// 资源路径分段需要百分号编码的字符集合：只保留 RFC 3986 的 unreserved 字符。
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// 需要改写的媒体资源选择器及对应属性。
const MEDIA_TARGETS: [(&str, &str); 4] = [
    ("img[src]", "src"),
    ("audio[src]", "src"),
    ("source[src]", "src"),
    ("video[poster]", "poster"),
];

/// 改写并清洗后的 HTML 与资源引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteResult {
    pub html: String,
    pub resources: Vec<ResourceRef>,
}

/// 生成 `mdx-res://{dict_id}/{percent-encoded path}` 逻辑 URI。
///
/// `path` 按 `/`（兼容 `\`）分段后逐段百分号编码；包含 `..` 等非法路径时返回空串。
pub fn resource_uri(dict_id: &str, path: &str) -> String {
    match relative_segments(path) {
        Some(segments) => format!("mdx-res://{dict_id}/{}", encode_segments(&segments)),
        None => String::new(),
    }
}

/// 改写相对资源路径并用 ammonia 白名单清洗。
pub fn rewrite_and_sanitize(raw_html: &str, dict_id: &str) -> Result<RewriteResult, ParserError> {
    ensure_acceptable_size(raw_html)?;

    let dict_id = Rc::new(dict_id.to_string());
    let resources = Rc::new(RefCell::new(Vec::<ResourceRef>::new()));
    let mut element_content_handlers = Vec::new();

    for (selector, attribute) in MEDIA_TARGETS {
        let handler_dict_id = Rc::clone(&dict_id);
        let handler_resources = Rc::clone(&resources);
        element_content_handlers.push((
            Cow::Owned(parse_selector(selector)?),
            ElementContentHandlers::default().element(move |element: &mut Element| {
                rewrite_media_element(element, attribute, &handler_dict_id, &handler_resources)
            }),
        ));
    }

    element_content_handlers.push((
        Cow::Owned(parse_selector("a[href]")?),
        ElementContentHandlers::default().element(rewrite_anchor_element),
    ));

    let rewritten = rewrite_str(
        raw_html,
        RewriteStrSettings {
            element_content_handlers,
            ..RewriteStrSettings::new()
        },
    )
    .map_err(|error| ParserError::InvalidData(format!("HTML 改写失败: {error}")))?;

    let resources = match Rc::try_unwrap(resources) {
        Ok(cell) => cell.into_inner(),
        Err(shared) => shared
            .try_borrow()
            .map(|resources| resources.clone())
            .unwrap_or_default(),
    };

    Ok(RewriteResult {
        html: sanitize_html(&rewritten),
        resources,
    })
}

/// 只做白名单清洗，供 Anki 导出使用。
pub fn sanitize(raw_html: &str) -> Result<String, ParserError> {
    ensure_acceptable_size(raw_html)?;
    Ok(sanitize_html(raw_html))
}

/// 校验单条 HTML 的字节上限。
fn ensure_acceptable_size(raw_html: &str) -> Result<(), ParserError> {
    if raw_html.len() > MAX_ENTRY_HTML_BYTES {
        return Err(ParserError::InvalidData(format!(
            "单条 HTML 超过上限: {} 字节 > {} 字节",
            raw_html.len(),
            MAX_ENTRY_HTML_BYTES
        )));
    }
    Ok(())
}

/// 解析静态选择器；选择器均为常量，解析失败只可能来自实现变更。
fn parse_selector(selector: &str) -> Result<Selector, ParserError> {
    selector
        .parse::<Selector>()
        .map_err(|error| ParserError::InvalidData(format!("HTML 选择器无效: {error}")))
}

/// 改写单个媒体元素的 `src`/`poster`。
fn rewrite_media_element(
    element: &mut Element,
    attribute: &str,
    dict_id: &str,
    resources: &RefCell<Vec<ResourceRef>>,
) -> HandlerResult {
    let Some(value) = element.get_attribute(attribute) else {
        return Ok(());
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        element.remove_attribute(attribute);
        return Ok(());
    }

    if has_http_scheme(trimmed) {
        // 远程资源保留原值，交给前端直接加载。
        push_unique(
            resources,
            ResourceRef::Remote {
                url: trimmed.to_string(),
            },
        );
        return Ok(());
    }

    if has_scheme(trimmed, "data:") {
        // data: 图片/音频保留内联内容，不登记资源。
        return Ok(());
    }

    rewrite_relative_resource(element, attribute, trimmed, dict_id, resources)
}

/// 把相对路径改写为 `mdx-res://` 逻辑 URI，并登记本地资源。
fn rewrite_relative_resource(
    element: &mut Element,
    attribute: &str,
    value: &str,
    dict_id: &str,
    resources: &RefCell<Vec<ResourceRef>>,
) -> HandlerResult {
    let Some(segments) = relative_segments(value) else {
        // 非法路径（如包含 `..`）直接移除属性，避免目录穿越。
        element.remove_attribute(attribute);
        return Ok(());
    };

    let uri = format!("mdx-res://{dict_id}/{}", encode_segments(&segments));
    element.set_attribute(attribute, &uri)?;

    let path = segments.join("/");
    push_unique(
        resources,
        ResourceRef::Local {
            dict_id: dict_id.to_string(),
            path,
        },
    );
    Ok(())
}

/// 处理 `a[href]`：移除 entry/javascript/data 链接，其余补 `rel`。
fn rewrite_anchor_element(element: &mut Element) -> HandlerResult {
    let Some(href) = element.get_attribute("href") else {
        return Ok(());
    };
    let value = href.trim();
    if value.is_empty()
        || has_scheme(value, "entry:")
        || has_scheme(value, "javascript:")
        || has_scheme(value, "data:")
    {
        element.remove_attribute("href");
        return Ok(());
    }

    // 链接本身不登记为待下载资源，只补安全属性；http 媒体资源在其他 handler 中登记。
    let rel = element.get_attribute("rel").unwrap_or_default();
    let mut tokens: Vec<String> = rel
        .split_whitespace()
        .map(|token| token.to_string())
        .collect();
    for required in ["noopener", "noreferrer"] {
        if !tokens
            .iter()
            .any(|token| token.eq_ignore_ascii_case(required))
        {
            tokens.push(required.to_string());
        }
    }
    if !tokens.is_empty() {
        element.set_attribute("rel", &tokens.join(" "))?;
    }
    Ok(())
}

/// 去重后登记资源，保持首次出现顺序。
fn push_unique(resources: &RefCell<Vec<ResourceRef>>, resource: ResourceRef) {
    let Ok(mut resources) = resources.try_borrow_mut() else {
        return;
    };
    if !resources.contains(&resource) {
        resources.push(resource);
    }
}

/// 是否以 http/https 开头（大小写不敏感）。
fn has_http_scheme(value: &str) -> bool {
    has_scheme(value, "http://") || has_scheme(value, "https://")
}

/// 判断前缀是否为给定 scheme，大小写不敏感且不会在非字符边界切片。
fn has_scheme(value: &str, scheme: &str) -> bool {
    value
        .get(..scheme.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
}

/// 按 `/` 与 `\` 分段；拒绝 `..`、丢弃 `.` 与空段，并拒绝空路径。
fn relative_segments(path: &str) -> Option<Vec<&str>> {
    let mut segments = Vec::new();
    for segment in path.split(['/', '\\']) {
        match segment {
            "" | "." => continue,
            ".." => return None,
            other => segments.push(other),
        }
    }
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

/// 对每个路径分段做 UTF-8 百分号编码后用 `/` 连接。
fn encode_segments(segments: &[&str]) -> String {
    segments
        .iter()
        .map(|segment| utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// ammonia 白名单清洗。
fn sanitize_html(raw_html: &str) -> String {
    let mut builder = ammonia::Builder::default();
    let mut url_schemes = HashSet::new();
    url_schemes.insert("http");
    url_schemes.insert("https");
    url_schemes.insert("mdx-res");
    // data: 仅在媒体标签上放行，由 attribute_filter 兜底限制。
    url_schemes.insert("data");

    builder
        .add_tags(&[
            "audio",
            "video",
            "source",
            "font",
            "tfoot",
            "figure",
            "figcaption",
            "track",
            "center",
        ])
        .add_generic_attributes(&["class"])
        .add_tag_attributes("audio", &["src", "controls", "preload", "loop", "muted"])
        .add_tag_attributes(
            "video",
            &[
                "src", "poster", "controls", "width", "height", "preload", "loop", "muted",
            ],
        )
        .add_tag_attributes("source", &["src", "type"])
        .add_tag_attributes("font", &["color", "face", "size"])
        .url_schemes(url_schemes)
        .attribute_filter(|element, attribute, value| {
            let is_url_attribute = matches!(
                attribute,
                "href"
                    | "src"
                    | "poster"
                    | "action"
                    | "data"
                    | "formaction"
                    | "ping"
                    | "xlink:href"
            );
            let media_element = matches!(element, "img" | "audio" | "video" | "source");
            if is_url_attribute && has_scheme(value, "data:") && !media_element {
                None
            } else {
                Some(Cow::Borrowed(value))
            }
        });

    builder.clean(raw_html).to_string()
}
