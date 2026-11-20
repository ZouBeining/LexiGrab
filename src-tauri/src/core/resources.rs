// SPDX-License-Identifier: AGPL-3.0-only
//! `mdx-res` resource protocol request parsing and resource resolution.
//!
//! The renderer encodes local dictionary media as
//! `mdx-res://{dict_id}/{percent-encoded path}` (see [`crate::core::html`]).
//! Tauri exposes custom schemes differently per platform, so the protocol
//! handler may receive any of these forms:
//!
//! - `mdx-res://{dict_id}/{encoded path}` (custom scheme, full URI);
//! - `mdx-res://localhost/{dict_id}/{encoded path}` (Windows handler form);
//! - `http://mdx-res.localhost/{dict_id}/{encoded path}` (Windows display form);
//! - `/{dict_id}/{encoded path}` (bare path, e.g. `request.uri().path()`).
//!
//! Wry serves custom schemes on Windows through the `*.localhost` workaround
//! and reverts `http://mdx-res.localhost/x` back to `mdx-res://localhost/x`
//! before invoking the handler, so the `localhost` authority is not the
//! dictionary id and must be skipped. Dictionary ids are generated as
//! `{source_id}-f{index}`, so they can never collide with that authority.
//!
//! Parsing happens before decoding: the raw path is split on `/` and every
//! segment is percent-decoded afterwards, so an encoded `/` or `\` can never
//! merge segments. Anything that smells like traversal, an empty segment or a
//! non-whitelisted MIME type is rejected instead of being resolved.

use percent_encoding::percent_decode_str;

use crate::core::error::{ParserError, SourceError};
use crate::core::registry::SourceRegistry;
use crate::sources::SourceResource;

/// Logical URI scheme used in definition HTML.
pub const SCHEME: &str = "mdx-res";

/// Upper bound for the decoded request path, in bytes.
const MAX_PATH_BYTES: usize = 4096;
/// Upper bound for the number of `/`-separated segments (including dict id).
const MAX_SEGMENTS: usize = 64;

/// MIME types the protocol is allowed to serve.
///
/// Only media categories needed by dictionary entries are listed: images,
/// audio, video, fonts and CSS. A curated table (instead of a `type/*` match)
/// keeps the response `Content-Type` a `&'static str` and stays conservative:
/// unknown or exotic formats are refused rather than sniffed.
const ALLOWED_MIMES: &[&str] = &[
    // Images.
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/svg+xml",
    "image/tiff",
    "image/x-icon",
    "image/vnd.microsoft.icon",
    "image/apng",
    "image/avif",
    "image/heic",
    "image/heif",
    "image/jp2",
    "image/jxl",
    "image/pict",
    "image/x-cmu-raster",
    "image/x-portable-anymap",
    "image/x-portable-bitmap",
    "image/x-portable-graymap",
    "image/x-portable-pixmap",
    "image/x-rgb",
    "image/x-tga",
    "image/x-xbitmap",
    "image/x-xpixmap",
    "image/vnd.wap.wbmp",
    // Audio.
    "audio/aac",
    "audio/aiff",
    "audio/basic",
    "audio/flac",
    "audio/m4a",
    "audio/m4b",
    "audio/mid",
    "audio/midi",
    "audio/mp4",
    "audio/mpeg",
    "audio/ogg",
    "audio/wav",
    "audio/webm",
    "audio/x-matroska",
    "audio/x-ms-wma",
    // Video.
    "video/3gpp",
    "video/3gpp2",
    "video/divx",
    "video/h264",
    "video/jpeg",
    "video/mj2",
    "video/mp4",
    "video/mpeg",
    "video/ogg",
    "video/quicktime",
    "video/webm",
    "video/x-dv",
    "video/x-f4v",
    "video/x-fli",
    "video/x-flv",
    "video/x-ivf",
    "video/x-m4v",
    "video/x-matroska",
    "video/x-mng",
    "video/x-ms-asf",
    "video/x-ms-vob",
    "video/x-ms-wm",
    "video/x-ms-wmp",
    "video/x-ms-wmv",
    "video/x-ms-wmx",
    "video/x-ms-wvx",
    "video/x-msvideo",
    "video/x-sgi-movie",
    // Fonts (including legacy `application/*` spellings returned by mime_guess).
    "font/collection",
    "font/otf",
    "font/ttf",
    "font/woff",
    "font/woff2",
    "application/font-sfnt",
    "application/font-woff",
    "application/vnd.ms-fontobject",
    "application/x-font-otf",
    "application/x-font-ttf",
    "application/x-font-woff",
    // Text.
    "text/css",
];

/// A resource request that passed syntax validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedResource {
    /// Logical dictionary id, e.g. `local-mini-f1`.
    pub dict_id: String,
    /// Decoded logical resource path, e.g. `img/hello.png`.
    pub path: String,
}

/// Why a resource request could not be served.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    /// The request is malformed (traversal attempt, bad encoding, too long, ...).
    #[error("Invalid resource request: {0}")]
    BadRequest(String),
    /// The dictionary or the resource inside it does not exist.
    #[error("Resource not found")]
    NotFound,
    /// The resource exists but its MIME type is not on the whitelist.
    #[error("Resource MIME type is not allowed: {0}")]
    ForbiddenMime(String),
}

impl ResourceError {
    /// HTTP status code the protocol handler should answer with.
    pub fn http_status(&self) -> u16 {
        match self {
            Self::BadRequest(_) => 400,
            Self::ForbiddenMime(_) => 403,
            Self::NotFound => 404,
        }
    }
}

/// Parse a protocol request path into a dictionary id and a decoded path.
///
/// Accepts the custom scheme (including the Windows `mdx-res://localhost/...`
/// handler form), the Windows `*.localhost` form, or the bare path
/// (`/{dict_id}/{encoded path}`) with an optional leading slash. Query and
/// fragment parts are ignored.
pub fn parse_request(raw_path: &str) -> Result<ParsedResource, ResourceError> {
    let path_part = strip_uri_prefix(raw_path);
    let path_part = strip_query_and_fragment(path_part);
    if path_part.is_empty() {
        return Err(ResourceError::BadRequest("empty resource path".to_string()));
    }
    if path_part.len() > MAX_PATH_BYTES {
        return Err(ResourceError::BadRequest(format!(
            "resource path exceeds {MAX_PATH_BYTES} bytes"
        )));
    }

    let without_leading_slash = match path_part.strip_prefix('/') {
        Some(rest) => rest,
        None => path_part,
    };
    if without_leading_slash.is_empty() {
        return Err(ResourceError::BadRequest(
            "resource path has no dictionary id".to_string(),
        ));
    }

    let raw_segments: Vec<&str> = without_leading_slash.split('/').collect();
    if raw_segments.len() > MAX_SEGMENTS {
        return Err(ResourceError::BadRequest(format!(
            "resource path exceeds {MAX_SEGMENTS} segments"
        )));
    }

    let mut decoded = Vec::with_capacity(raw_segments.len());
    for raw_segment in raw_segments {
        if raw_segment.is_empty() {
            return Err(ResourceError::BadRequest(
                "resource path contains an empty segment".to_string(),
            ));
        }
        if has_invalid_percent_encoding(raw_segment) {
            return Err(ResourceError::BadRequest(
                "resource path contains invalid percent-encoding".to_string(),
            ));
        }
        let bytes = percent_decode_str(raw_segment).collect::<Vec<u8>>();
        let segment = String::from_utf8(bytes).map_err(|_| {
            ResourceError::BadRequest("resource path is not valid UTF-8".to_string())
        })?;
        decoded.push(segment);
    }
    validate_decoded_segments(&decoded)?;

    if decoded.len() < 2 {
        return Err(ResourceError::BadRequest(
            "resource path has no resource segment".to_string(),
        ));
    }

    let dict_id = decoded.remove(0);
    let path = decoded.join("/");
    if path.len() > MAX_PATH_BYTES {
        return Err(ResourceError::BadRequest(format!(
            "decoded resource path exceeds {MAX_PATH_BYTES} bytes"
        )));
    }

    Ok(ParsedResource { dict_id, path })
}

/// Guess the MIME type of a logical path and return it only when whitelisted.
pub fn mime_for_path(path: &str) -> Option<&'static str> {
    let guessed = mime_guess::from_path(path).first()?;
    let essence = guessed.essence_str();
    ALLOWED_MIMES
        .iter()
        .copied()
        .find(|allowed| allowed.eq_ignore_ascii_case(essence))
}

/// Resolve a parsed request through the enabled data sources.
///
/// Sources that do not own `parsed.dict_id` are skipped; once the owning
/// source reports `RESOURCE_NOT_FOUND` the error is final. A resource whose
/// MIME type is not whitelisted is rejected with [`ResourceError::ForbiddenMime`].
pub fn resolve_resource(
    registry: &SourceRegistry,
    parsed: &ParsedResource,
) -> Result<SourceResource, ResourceError> {
    if parsed.dict_id.is_empty() || parsed.path.is_empty() {
        return Err(ResourceError::BadRequest(
            "resource request is incomplete".to_string(),
        ));
    }
    let mut segments = Vec::with_capacity(2 + parsed.path.matches('/').count());
    segments.push(parsed.dict_id.clone());
    segments.extend(parsed.path.split('/').map(str::to_string));
    validate_decoded_segments(&segments)?;

    for source in registry.enabled_sources() {
        match source.resolve_resource(&parsed.dict_id, &parsed.path) {
            Ok(resource) => {
                if is_allowed_mime(&resource.mime) {
                    return Ok(resource);
                }
                return Err(ResourceError::ForbiddenMime(resource.mime));
            }
            // The owning source has no such resource: stop, other sources
            // cannot answer for the same dictionary id.
            Err(SourceError::Parser(ParserError::ResourceNotFound(_))) => {
                return Err(ResourceError::NotFound);
            }
            Err(SourceError::Parser(ParserError::InvalidData(message))) => {
                return Err(ResourceError::BadRequest(message));
            }
            // `Internal` is how a local source reports "unknown dict id";
            // other transient failures are also skipped here.
            Err(_) => continue,
        }
    }

    Err(ResourceError::NotFound)
}

/// True when a MIME essence string is on the protocol whitelist.
fn is_allowed_mime(mime: &str) -> bool {
    let essence = mime.split(';').next().unwrap_or_default().trim();
    ALLOWED_MIMES
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(essence))
}

/// Strip a supported URI prefix, if any, leaving the shared path form.
///
/// The custom-scheme form additionally drops a leading `localhost` authority,
/// which is what Wry hands to the Windows protocol handler. The `*.localhost`
/// display form keeps its own host as part of the stripped prefix.
fn strip_uri_prefix(raw: &str) -> &str {
    let custom = format!("{SCHEME}://");
    if let Some(rest) = strip_prefix_ascii_case_insensitive(raw, &custom) {
        return strip_localhost_authority(rest);
    }
    for host in ["http://mdx-res.localhost", "https://mdx-res.localhost"] {
        if let Some(rest) = strip_prefix_ascii_case_insensitive(raw, host) {
            return rest;
        }
    }
    raw
}

/// Drop a `localhost` URI authority from `{authority}/{dict_id}/{path}`.
///
/// Only the exact host name is removed; a dictionary id such as
/// `localhost-f1` is left untouched because it has no `/` right after the
/// name.
fn strip_localhost_authority(rest: &str) -> &str {
    let trimmed = rest.strip_prefix('/').unwrap_or(rest);
    let Some(remainder) = strip_prefix_ascii_case_insensitive(trimmed, "localhost") else {
        return rest;
    };
    match remainder.strip_prefix('/') {
        Some(tail) => tail,
        // `mdx-res://localhost` without a path stays invalid.
        None if remainder.is_empty() => remainder,
        None => rest,
    }
}

/// Case-insensitive `strip_prefix` that never splits UTF-8 boundaries.
fn strip_prefix_ascii_case_insensitive<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

/// Drop `?query` and `#fragment` suffixes.
fn strip_query_and_fragment(value: &str) -> &str {
    let end = value.find(['?', '#']).unwrap_or(value.len());
    &value[..end]
}

/// Reject `%` sequences that are not exactly two hex digits.
fn has_invalid_percent_encoding(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let valid = bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit);
            if !valid {
                return true;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    false
}

/// Validate already-decoded segments: no emptiness, dot segments, separators or
/// control characters. Used both while parsing and as defence in depth when a
/// [`ParsedResource`] is resolved directly.
fn validate_decoded_segments(segments: &[String]) -> Result<(), ResourceError> {
    if segments.is_empty() {
        return Err(ResourceError::BadRequest(
            "resource path is empty".to_string(),
        ));
    }
    for segment in segments {
        if segment.is_empty() {
            return Err(ResourceError::BadRequest(
                "resource path contains an empty segment".to_string(),
            ));
        }
        if segment == "." || segment == ".." {
            return Err(ResourceError::BadRequest(
                "resource path contains a traversal segment".to_string(),
            ));
        }
        if segment.contains('/') || segment.contains('\\') {
            return Err(ResourceError::BadRequest(
                "resource path segment contains a separator".to_string(),
            ));
        }
        if segment.chars().any(char::is_control) {
            return Err(ResourceError::BadRequest(
                "resource path contains a control character".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_custom_scheme_uri() {
        let parsed = parse_request("mdx-res://local-mini-f1/img/hello.png").expect("parse");
        assert_eq!(parsed.dict_id, "local-mini-f1");
        assert_eq!(parsed.path, "img/hello.png");
    }

    #[test]
    fn parses_windows_localhost_uri() {
        for scheme in ["http", "https"] {
            let raw = format!("{scheme}://mdx-res.localhost/local-mini-f1/img/hello.png");
            let parsed = parse_request(&raw).expect("parse");
            assert_eq!(parsed.dict_id, "local-mini-f1");
            assert_eq!(parsed.path, "img/hello.png");
        }
    }

    #[test]
    fn parses_custom_scheme_with_localhost_authority() {
        // Wry reverts `http://mdx-res.localhost/...` to this form before the
        // Windows custom-protocol handler runs.
        for raw in [
            "mdx-res://localhost/local-mini-f1/img/hello.png",
            "mdx-res://LOCALHOST/local-mini-f1/img/hello.png",
            "mdx-res://localhost/local-mini-f1/img/hello.png?cache=1",
        ] {
            let parsed = parse_request(raw).expect("parse");
            assert_eq!(parsed.dict_id, "local-mini-f1", "{raw}");
            assert_eq!(parsed.path, "img/hello.png", "{raw}");
        }
    }

    #[test]
    fn does_not_mistake_dict_id_prefixed_with_localhost_for_authority() {
        let parsed = parse_request("mdx-res://localhost-f1/img/hello.png").expect("parse");
        assert_eq!(parsed.dict_id, "localhost-f1");
        assert_eq!(parsed.path, "img/hello.png");
    }

    #[test]
    fn rejects_localhost_authority_without_resource_path() {
        for raw in [
            "mdx-res://localhost",
            "mdx-res://localhost/",
            "mdx-res://localhost/local-mini-f1",
            "mdx-res://localhost/local-mini-f1/",
        ] {
            let error = parse_request(raw).expect_err("incomplete resource path");
            assert_eq!(error.http_status(), 400, "{raw}");
        }
    }

    #[test]
    fn parses_bare_path_with_or_without_leading_slash() {
        for raw in [
            "/local-mini-f1/img/hello.png",
            "local-mini-f1/img/hello.png",
        ] {
            let parsed = parse_request(raw).expect("parse");
            assert_eq!(parsed.dict_id, "local-mini-f1");
            assert_eq!(parsed.path, "img/hello.png");
        }
    }

    #[test]
    fn strips_query_and_fragment() {
        let parsed = parse_request("mdx-res://d/a.png?cache=1#frag").expect("parse");
        assert_eq!(parsed.dict_id, "d");
        assert_eq!(parsed.path, "a.png");
    }

    #[test]
    fn decodes_percent_encoded_unicode_and_spaces() {
        let parsed = parse_request("mdx-res://d/%E4%BD%A0%E5%A5%BD%20world.png").expect("parse");
        assert_eq!(parsed.dict_id, "d");
        assert_eq!(parsed.path, "你好 world.png");
    }

    #[test]
    fn decodes_after_splitting_segments() {
        let parsed = parse_request("mdx-res://d/img/%68ello.png").expect("parse");
        assert_eq!(parsed.path, "img/hello.png");
    }

    #[test]
    fn rejects_traversal_and_dot_segments() {
        for raw in [
            "mdx-res://d/../secret",
            "mdx-res://d/%2e%2e/secret",
            "mdx-res://d/%2E%2E/secret",
            "mdx-res://d/img/..",
            "mdx-res://d/./a.png",
            "mdx-res://d/a%2Fb.png",
            "mdx-res://d/a%5Cb.png",
        ] {
            assert!(
                parse_request(raw).is_err(),
                "{raw} should be rejected as traversal"
            );
        }
    }

    #[test]
    fn rejects_backslash_empty_segments_and_bare_id() {
        for raw in [
            "mdx-res://d/img\\a.png",
            "mdx-res://d/a//b.png",
            "mdx-res://d/a.png/",
            "mdx-res://d",
            "mdx-res://d/",
            "mdx-res://",
            "",
        ] {
            let error = parse_request(raw).expect_err("should be rejected");
            assert_eq!(error.http_status(), 400, "{raw}");
        }
    }

    #[test]
    fn rejects_invalid_percent_encoding_and_utf8() {
        for raw in [
            "mdx-res://d/%zz.png",
            "mdx-res://d/%2.png",
            "mdx-res://d/%FF.png",
        ] {
            assert!(parse_request(raw).is_err(), "{raw} should be rejected");
        }
    }

    #[test]
    fn rejects_overlong_path() {
        let long = format!("mdx-res://d/{}", "a".repeat(MAX_PATH_BYTES));
        assert_eq!(
            parse_request(&long).expect_err("too long").http_status(),
            400
        );
    }

    #[test]
    fn rejects_too_many_segments() {
        let raw = format!("mdx-res://d/{}", vec!["a"; MAX_SEGMENTS].join("/"));
        assert_eq!(
            parse_request(&raw)
                .expect_err("too many segments")
                .http_status(),
            400
        );
    }

    #[test]
    fn accepts_segment_count_at_limit() {
        let raw = format!("mdx-res://d/{}", vec!["a"; MAX_SEGMENTS - 1].join("/"));
        assert!(parse_request(&raw).is_ok());
    }

    #[test]
    fn mime_whitelist_allows_media_and_css() {
        for (path, expected) in [
            ("img/hello.png", "image/png"),
            ("audio/hello.mp3", "audio/mpeg"),
            ("audio/hello.wav", "audio/wav"),
            ("font/hello.woff2", "font/woff2"),
            ("style/entry.css", "text/css"),
            ("img/hello.svg", "image/svg+xml"),
        ] {
            assert_eq!(mime_for_path(path), Some(expected), "{path}");
        }
    }

    #[test]
    fn mime_whitelist_rejects_other_types() {
        for path in [
            "script.js",
            "page.html",
            "binary.exe",
            "archive.zip",
            "unknown",
        ] {
            assert!(mime_for_path(path).is_none(), "{path}");
        }
    }

    #[test]
    fn mime_whitelist_allows_legacy_font_types() {
        // `eot` is a font but mime_guess maps it to a legacy `application/*`
        // type; the whitelist still accepts it.
        assert_eq!(
            mime_for_path("font.eot"),
            Some("application/vnd.ms-fontobject")
        );
    }

    #[test]
    fn empty_registry_resolves_to_not_found() {
        let registry = SourceRegistry::empty();
        let parsed = ParsedResource {
            dict_id: "missing".to_string(),
            path: "img/a.png".to_string(),
        };
        assert_eq!(
            resolve_resource(&registry, &parsed).expect_err("no sources"),
            ResourceError::NotFound
        );
    }

    #[test]
    fn resolve_rejects_unsafe_parsed_paths() {
        let registry = SourceRegistry::empty();
        for path in ["../secret", "a//b", "a\\b", "a/./b", ""] {
            let parsed = ParsedResource {
                dict_id: "d".to_string(),
                path: path.to_string(),
            };
            let error = resolve_resource(&registry, &parsed).expect_err("unsafe path");
            assert!(matches!(error, ResourceError::BadRequest(_)), "{path}");
            assert_eq!(error.http_status(), 400, "{path}");
        }
    }

    #[test]
    fn error_status_codes_match_contract() {
        assert_eq!(ResourceError::BadRequest("x".into()).http_status(), 400);
        assert_eq!(
            ResourceError::ForbiddenMime("text/html".into()).http_status(),
            403
        );
        assert_eq!(ResourceError::NotFound.http_status(), 404);
    }
}
