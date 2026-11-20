// SPDX-License-Identifier: AGPL-3.0-only
//! T1.5 HTML 清洗与资源改写的黄金用例。

use lexigrab_lib::core::entry::ResourceRef;
use lexigrab_lib::core::error::ParserError;
use lexigrab_lib::core::html::{
    resource_uri, rewrite_and_sanitize, sanitize, RewriteResult, MAX_ENTRY_HTML_BYTES,
};

fn rewrite(html: &str) -> RewriteResult {
    match rewrite_and_sanitize(html, "dict-1") {
        Ok(result) => result,
        Err(error) => panic!("改写失败: {error}"),
    }
}

#[test]
fn removes_scripts_and_event_handlers() {
    let result = rewrite(
        r#"<p>hi</p><script>alert(1)</script><img src="a.png" onerror="alert(2)" onclick="x()">"#,
    );

    assert!(result.html.contains("<p>hi</p>"));
    assert!(!result.html.contains("<script"));
    assert!(!result.html.contains("alert(1)"));
    assert!(!result.html.contains("onerror"));
    assert!(!result.html.contains("onclick"));
    assert!(result.html.contains(r#"src="mdx-res://dict-1/a.png""#));
    assert_eq!(
        result.resources,
        vec![ResourceRef::Local {
            dict_id: "dict-1".to_string(),
            path: "a.png".to_string(),
        }]
    );
}

#[test]
fn removes_javascript_links_and_keeps_safe_links() {
    let result = rewrite(
        r#"<a href="javascript:alert(1)">bad</a><a href="https://example.com/page">ok</a>"#,
    );

    assert!(!result.html.contains("javascript:"));
    assert!(!result.html.contains(r#"href="javascript"#));
    assert!(result.html.contains(r#"href="https://example.com/page""#));
    assert!(result.html.contains(r#"rel="noopener noreferrer""#));
}

#[test]
fn removes_entry_links() {
    let result = rewrite(r#"<a href="entry://hello">hello</a>"#);

    assert!(!result.html.contains("entry://"));
    assert!(!result.html.contains("href"));
    assert!(result.html.contains("hello"));
}

#[test]
fn rewrites_relative_media_with_spaces_chinese_and_backslash() {
    let result = rewrite(r#"<img src="img\sub dir/中文 图片.png" alt="样例">"#);

    assert_eq!(
        result.resources,
        vec![ResourceRef::Local {
            dict_id: "dict-1".to_string(),
            path: "img/sub dir/中文 图片.png".to_string(),
        }]
    );
    assert!(result.html.contains(
        r#"src="mdx-res://dict-1/img/sub%20dir/%E4%B8%AD%E6%96%87%20%E5%9B%BE%E7%89%87.png""#
    ));
    assert!(result.html.contains(r#"alt="样例""#));
}

#[test]
fn rewrites_audio_source_and_video_poster() {
    let result = rewrite(
        r#"<audio src="audio/a.mp3" controls></audio><video poster="img/p.jpg" controls><source src="audio/a.mp3" type="audio/mpeg"></video>"#,
    );

    assert!(result
        .html
        .contains(r#"src="mdx-res://dict-1/audio/a.mp3""#));
    assert!(result
        .html
        .contains(r#"poster="mdx-res://dict-1/img/p.jpg""#));
    assert_eq!(
        result.resources,
        vec![
            ResourceRef::Local {
                dict_id: "dict-1".to_string(),
                path: "audio/a.mp3".to_string(),
            },
            ResourceRef::Local {
                dict_id: "dict-1".to_string(),
                path: "img/p.jpg".to_string(),
            },
        ]
    );
}

#[test]
fn keeps_remote_images_and_collects_them() {
    let result = rewrite(r#"<img src="https://cdn.example.com/a.png">"#);

    assert!(result
        .html
        .contains(r#"src="https://cdn.example.com/a.png""#));
    assert_eq!(
        result.resources,
        vec![ResourceRef::Remote {
            url: "https://cdn.example.com/a.png".to_string(),
        }]
    );
}

#[test]
fn keeps_data_images_without_collecting_resources() {
    let data_uri = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg==";
    let html = format!(r#"<img src="{data_uri}">"#);
    let result = rewrite(&html);

    assert!(result.html.contains(data_uri));
    assert!(result.resources.is_empty());
}

#[test]
fn deduplicates_resources_and_keeps_first_seen_order() {
    let result = rewrite(
        r#"<img src="a.png"><img src="./a.png"><img src="https://x.example/a.png"><img src="https://x.example/a.png"><audio src="a.png"></audio>"#,
    );

    assert_eq!(
        result.resources,
        vec![
            ResourceRef::Local {
                dict_id: "dict-1".to_string(),
                path: "a.png".to_string(),
            },
            ResourceRef::Remote {
                url: "https://x.example/a.png".to_string(),
            },
        ]
    );
}

#[test]
fn rejects_parent_directory_traversal() {
    let result = rewrite(r#"<img src="../secret.png"><a href="../other">link</a>"#);

    assert!(!result.html.contains("../secret.png"));
    assert!(result.resources.is_empty());
}

#[test]
fn oversized_html_returns_invalid_data() {
    let oversized = "a".repeat(MAX_ENTRY_HTML_BYTES + 1);

    assert!(matches!(
        rewrite_and_sanitize(&oversized, "dict-1"),
        Err(ParserError::InvalidData(_))
    ));
    assert!(matches!(
        sanitize(&oversized),
        Err(ParserError::InvalidData(_))
    ));
}

#[test]
fn sanitize_strips_dangerous_markup_without_rewriting() {
    let cleaned = match sanitize(
        r#"<p style="color:red" onclick="x()">hi<script>bad()</script><a href="javascript:alert(1)">x</a></p>"#,
    ) {
        Ok(cleaned) => cleaned,
        Err(error) => panic!("清洗失败: {error}"),
    };

    assert!(!cleaned.contains("style="));
    assert!(!cleaned.contains("onclick"));
    assert!(!cleaned.contains("<script"));
    assert!(!cleaned.contains("javascript:"));
    assert!(cleaned.contains("<p>"));
    assert!(cleaned.contains("hi"));
    assert!(cleaned.contains("</p>"));
}

#[test]
fn sanitize_allows_formatting_and_adds_link_rel() {
    let cleaned = match sanitize(
        r#"<div><b>粗体</b><span class="note">注释</span><a href="https://example.com">链接</a></div>"#,
    ) {
        Ok(cleaned) => cleaned,
        Err(error) => panic!("清洗失败: {error}"),
    };

    assert!(cleaned.contains("<b>粗体</b>"));
    assert!(cleaned.contains(r#"class="note""#));
    assert!(cleaned.contains(r#"rel="noopener noreferrer""#));
}

#[test]
fn sanitize_removes_unsupported_schemes() {
    let cleaned = match sanitize(
        r#"<a href="mailto:someone@example.com">mail</a><a href="data:text/html,hi">data</a>"#,
    ) {
        Ok(cleaned) => cleaned,
        Err(error) => panic!("清洗失败: {error}"),
    };

    assert!(!cleaned.contains("mailto:"));
    assert!(!cleaned.contains("data:text/html"));
}

#[test]
fn resource_uri_percent_encodes_each_segment() {
    assert_eq!(
        resource_uri("dict-2", r"img\子目录/a b.png"),
        "mdx-res://dict-2/img/%E5%AD%90%E7%9B%AE%E5%BD%95/a%20b.png"
    );
    assert_eq!(resource_uri("dict-2", "a.png"), "mdx-res://dict-2/a.png");
}
