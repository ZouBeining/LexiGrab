//! `mdx-res` protocol integration tests: exercise the M2 fixtures end to end,
//! from request parsing down to the resolved resource bytes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lexigrab_lib::core::entry::{SourceConfig, SourceKind};
use lexigrab_lib::core::error::SourceError;
use lexigrab_lib::core::registry::SourceRegistry;
use lexigrab_lib::core::resources::{
    mime_for_path, parse_request, resolve_resource, ParsedResource, ResourceError, SCHEME,
};
use lexigrab_lib::sources::local_file::LocalFileSource;
use lexigrab_lib::sources::{DictionarySource, SourceResource, SourceStatus};

const MINI_SOURCE_ID: &str = "local-mini";
const MINI_DICT_ID: &str = "local-mini-f1";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn mini_config(enabled: bool) -> SourceConfig {
    SourceConfig {
        id: MINI_SOURCE_ID.to_string(),
        name: "Mini Fixture".to_string(),
        kind: SourceKind::LocalFile {
            paths: vec![Path::new("tests/fixtures/mini.mdx").to_path_buf()],
        },
        enabled,
        priority: 10,
    }
}

/// Open the mini.mdx / mini.mdd fixtures through a real `LocalFileSource`.
fn mini_registry(enabled: bool) -> SourceRegistry {
    let source_config = mini_config(true);
    let source = Arc::new(LocalFileSource::open(source_config, &manifest_dir()));
    let mut registry = SourceRegistry::empty();
    registry.register(mini_config(enabled), source);
    registry
}

/// Fake source that deliberately returns a non-whitelisted MIME type.
struct ForbiddenMimeSource;

impl DictionarySource for ForbiddenMimeSource {
    fn id(&self) -> &str {
        "fake"
    }

    fn name(&self) -> &str {
        "Fake"
    }

    fn priority(&self) -> i32 {
        0
    }

    fn enabled(&self) -> bool {
        true
    }

    fn status(&self) -> SourceStatus {
        SourceStatus::Ready { entry_count: 0 }
    }

    fn lookup(&self, _query: &str) -> Result<Vec<lexigrab_lib::core::entry::Entry>, SourceError> {
        Ok(Vec::new())
    }

    fn resolve_resource(&self, _dict_id: &str, _path: &str) -> Result<SourceResource, SourceError> {
        Ok(SourceResource {
            bytes: b"<html></html>".to_vec(),
            mime: "text/html".to_string(),
        })
    }
}

fn fake_registry() -> SourceRegistry {
    let config = SourceConfig {
        id: "fake".to_string(),
        name: "Fake".to_string(),
        kind: SourceKind::LocalFile { paths: Vec::new() },
        enabled: true,
        priority: 0,
    };
    let mut registry = SourceRegistry::empty();
    registry.register(config, Arc::new(ForbiddenMimeSource));
    registry
}

#[test]
fn scheme_constant_matches_frontend_contract() {
    assert_eq!(SCHEME, "mdx-res");
}

#[test]
fn resolves_fixture_image_through_custom_scheme_uri() {
    let registry = mini_registry(true);
    let parsed = parse_request("mdx-res://local-mini-f1/img/hello.png").expect("parse custom uri");
    assert_eq!(parsed.dict_id, MINI_DICT_ID);
    assert_eq!(parsed.path, "img/hello.png");

    let resource = resolve_resource(&registry, &parsed).expect("resolve image");
    assert!(resource.bytes.starts_with(b"\x89PNG"), "not a PNG payload");
    assert_eq!(resource.mime, "image/png");
    assert_eq!(mime_for_path(&parsed.path), Some("image/png"));
}

#[test]
fn resolves_fixture_image_through_windows_localhost_uri() {
    let registry = mini_registry(true);
    let parsed = parse_request("http://mdx-res.localhost/local-mini-f1/img/hello.png")
        .expect("parse windows uri");
    assert_eq!(parsed.dict_id, MINI_DICT_ID);
    assert_eq!(parsed.path, "img/hello.png");

    let resource = resolve_resource(&registry, &parsed).expect("resolve image");
    assert!(resource.bytes.starts_with(b"\x89PNG"));
}

#[test]
fn resolves_fixture_audio_resource() {
    let registry = mini_registry(true);
    let parsed = parse_request("mdx-res://local-mini-f1/audio/hello.mp3").expect("parse");
    let resource = resolve_resource(&registry, &parsed).expect("resolve audio");
    assert_eq!(resource.mime, "audio/mpeg");
    assert_eq!(mime_for_path(&parsed.path), Some("audio/mpeg"));
}

#[test]
fn percent_encoded_request_path_still_resolves() {
    let registry = mini_registry(true);
    // `%68` decodes to `h`: decoding happens after splitting, so this is a
    // normal path, not a traversal attempt.
    let parsed = parse_request("mdx-res://local-mini-f1/img/%68ello.png").expect("parse");
    assert_eq!(parsed.path, "img/hello.png");
    let resource = resolve_resource(&registry, &parsed).expect("resolve encoded image");
    assert!(resource.bytes.starts_with(b"\x89PNG"));
}

#[test]
fn traversal_requests_are_bad_requests() {
    for raw in [
        "mdx-res://local-mini-f1/img/../secret",
        "mdx-res://local-mini-f1/img/%2e%2e/secret",
        "mdx-res://local-mini-f1/img%5C..%5Csecret",
    ] {
        let error = parse_request(raw).expect_err("traversal must be rejected");
        assert!(matches!(error, ResourceError::BadRequest(_)), "{raw}");
        assert_eq!(error.http_status(), 400, "{raw}");
    }
}

#[test]
fn missing_resource_maps_to_not_found() {
    let registry = mini_registry(true);
    let parsed = parse_request("mdx-res://local-mini-f1/img/missing.png").expect("parse");
    let error = resolve_resource(&registry, &parsed).expect_err("missing resource");
    assert_eq!(error, ResourceError::NotFound);
    assert_eq!(error.http_status(), 404);
}

#[test]
fn unknown_dictionary_maps_to_not_found() {
    let registry = mini_registry(true);
    let parsed = parse_request("mdx-res://local-unknown-f1/img/hello.png").expect("parse");
    let error = resolve_resource(&registry, &parsed).expect_err("unknown dict");
    assert_eq!(error, ResourceError::NotFound);
}

#[test]
fn disabled_source_is_not_served() {
    // Disabled sources are filtered out by `enabled_sources`: the protocol must
    // not leak resources from a dictionary the user turned off.
    let registry = mini_registry(false);
    let parsed = parse_request("mdx-res://local-mini-f1/img/hello.png").expect("parse");
    let error = resolve_resource(&registry, &parsed).expect_err("disabled source");
    assert_eq!(error, ResourceError::NotFound);
    assert_eq!(error.http_status(), 404);
}

#[test]
fn non_whitelisted_mime_is_forbidden() {
    let registry = fake_registry();
    let parsed = ParsedResource {
        dict_id: "fake".to_string(),
        path: "page.html".to_string(),
    };
    let error = resolve_resource(&registry, &parsed).expect_err("html is forbidden");
    assert!(
        matches!(error, ResourceError::ForbiddenMime(_)),
        "{error:?}"
    );
    assert_eq!(error.http_status(), 403);
}

#[test]
fn safely_built_parsed_resource_rejects_traversal() {
    // `parsed` is public for the command layer; `resolve_resource` re-validates
    // it instead of trusting callers.
    let registry = mini_registry(true);
    let parsed = ParsedResource {
        dict_id: MINI_DICT_ID.to_string(),
        path: "../secret".to_string(),
    };
    let error = resolve_resource(&registry, &parsed).expect_err("traversal");
    assert!(matches!(error, ResourceError::BadRequest(_)), "{error:?}");
    assert_eq!(error.http_status(), 400);
}

#[test]
fn invalid_utf8_path_is_bad_request() {
    let error = parse_request("mdx-res://local-mini-f1/img/%FF.png").expect_err("invalid utf8");
    assert_eq!(error.http_status(), 400);
}

#[test]
fn mime_whitelist_rejects_scripts_and_markup() {
    for path in ["script.js", "page.html", "binary.exe"] {
        assert!(mime_for_path(path).is_none(), "{path}");
    }
}
