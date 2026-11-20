// SPDX-License-Identifier: AGPL-3.0-only
//! Anki export pipeline: strip local dictionary media, sanitize, check the
//! saved-card context and create the note. Contract:
//! docs/BACKEND_STRUCTURE.md §6 and §3.
//!
//! Order matters: media stripping and sanitizing happen before the context
//! check so the fingerprint matches exactly what Anki receives. The context
//! check is advisory (it only feeds the popup warning) and never blocks an
//! export; `allowDuplicate: true` is always sent by the client.

use std::borrow::Cow;

use lol_html::html_content::{ContentType, Element};
use lol_html::{rewrite_str, ElementContentHandlers, HandlerResult, RewriteStrSettings, Selector};
use serde::Serialize;

use crate::anki::client::AnkiClient;
use crate::anki::mapping;
use crate::core::config::{AnkiSettings, DuplicatePolicy};
use crate::core::entry::AnkiPayload;
use crate::core::error::AnkiError;
use crate::db::CardStore;

pub use crate::db::fingerprint::definition_text;

/// Plain-text placeholder inserted for a stripped local image.
pub const IMAGE_PLACEHOLDER: &str = "[图片]";
/// Plain-text placeholder inserted for a stripped local audio element.
pub const AUDIO_PLACEHOLDER: &str = "[音频]";
/// Plain-text placeholder inserted for a stripped local video element.
pub const VIDEO_PLACEHOLDER: &str = "[视频]";

/// Logical media scheme produced by the dictionary HTML rewriter.
const MDX_RES_SCHEME: &str = "mdx-res://";
/// Host form used on Windows by the custom protocol handler.
const MDX_RES_WINDOWS_HOST: &str = "mdx-res.localhost/";

/// Result of a successful export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ExportOutcome {
    pub note_id: i64,
    /// Whether the exact word + sentence + definition context was already
    /// saved before this export (always false for `DuplicatePolicy::Allow`).
    pub exact_context_seen: bool,
}

/// Dependencies required to export one card.
pub struct ExportDeps<'a> {
    pub settings: &'a AnkiSettings,
    pub cards: &'a CardStore,
}

/// Remove local dictionary media from a definition and return sanitized HTML.
///
/// Local `mdx-res://` (and `mdx-res.localhost`) `img`/`audio`/`video` elements
/// are replaced with the plain-text placeholders `[图片]`/`[音频]`/`[视频]`;
/// local `source` children and `video` posters are dropped. Remote `http(s)`
/// media is kept. The result is ammonia-sanitized with the shared whitelist.
pub fn strip_local_media(html: &str) -> String {
    let rewritten = match rewrite_media(html) {
        Ok(rewritten) => rewritten,
        Err(error) => {
            tracing::warn!(error = %error, "failed to strip local media from definition");
            html.to_string()
        }
    };
    match crate::core::html::sanitize(&rewritten) {
        Ok(clean) => clean,
        Err(error) => {
            // The only sanitize failure is the 512KB input limit. Return the
            // rewritten HTML so `build_note_request` still rejects it instead
            // of silently exporting an empty definition.
            tracing::warn!(error = %error, "failed to sanitize exported definition");
            rewritten
        }
    }
}

/// Plain-text fingerprint input for a definition exactly as it is exported.
///
/// The popup's pre-save context check and the stored card fingerprint must
/// agree, so both derive the text from the media-stripped HTML: local images
/// and audio become `[图片]`/`[音频]` placeholders, which changes the text a
/// raw-HTML scraper would produce.
pub fn context_definition_text(html: &str) -> String {
    definition_text(&strip_local_media(html))
}

/// Full export: strip/sanitize -> context check -> AnkiConnect addNote -> record.
///
/// A failure to persist the saved-card row is logged but does not turn a
/// successful Anki export into an error.
pub fn export_to_anki(
    deps: ExportDeps<'_>,
    payload: &AnkiPayload,
) -> Result<ExportOutcome, AnkiError> {
    let definition_html = strip_local_media(&payload.definition_html);
    // Derive the fingerprint from the exact stripped HTML that is sent, so the
    // pre-send check and the post-send record always agree.
    let definition_plain = definition_text(&definition_html);
    let cleaned = AnkiPayload {
        word: payload.word.clone(),
        definition_html,
        sentence: payload.sentence.clone(),
        source: payload.source.clone(),
        tags: payload.tags.clone(),
    };

    let exact_context_seen = match deps.settings.duplicate_policy {
        DuplicatePolicy::Allow => false,
        DuplicatePolicy::Warn => match deps.cards.check_context(
            &cleaned.word,
            cleaned.sentence.as_deref(),
            &definition_plain,
        ) {
            Ok(context) => context.seen,
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "failed to check saved-card context; continuing export"
                );
                false
            }
        },
    };

    let client = AnkiClient::new(deps.settings.endpoint.clone())?;
    let available = client.model_field_names(&deps.settings.model)?;
    let request = mapping::build_note_request(deps.settings, &cleaned, &available)?;
    let note_id = client.add_note(&request)?;

    if let Err(error) = deps.cards.record_card(
        &cleaned.word,
        cleaned.sentence.as_deref(),
        &definition_plain,
        &cleaned.source,
        note_id,
    ) {
        tracing::warn!(
            error = %error,
            note_id,
            "failed to record exported card; export is still successful"
        );
    }

    Ok(ExportOutcome {
        note_id,
        exact_context_seen,
    })
}

/// Run lol_html over the definition with the media-stripping handlers.
fn rewrite_media(html: &str) -> Result<String, String> {
    let Some(img_selector) = parse_selector("img[src]") else {
        return Ok(html.to_string());
    };
    let Some(audio_selector) = parse_selector("audio[src]") else {
        return Ok(html.to_string());
    };
    let Some(video_selector) = parse_selector("video[src]") else {
        return Ok(html.to_string());
    };
    let Some(source_selector) = parse_selector("source[src]") else {
        return Ok(html.to_string());
    };
    let Some(poster_selector) = parse_selector("video[poster]") else {
        return Ok(html.to_string());
    };

    rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                (
                    Cow::Owned(img_selector),
                    ElementContentHandlers::default().element(|element: &mut Element| {
                        replace_when_local(element, "src", IMAGE_PLACEHOLDER)
                    }),
                ),
                (
                    Cow::Owned(audio_selector),
                    ElementContentHandlers::default().element(|element: &mut Element| {
                        replace_when_local(element, "src", AUDIO_PLACEHOLDER)
                    }),
                ),
                (
                    Cow::Owned(video_selector),
                    ElementContentHandlers::default().element(|element: &mut Element| {
                        replace_when_local(element, "src", VIDEO_PLACEHOLDER)
                    }),
                ),
                (
                    Cow::Owned(source_selector),
                    ElementContentHandlers::default().element(remove_when_local),
                ),
                (
                    Cow::Owned(poster_selector),
                    ElementContentHandlers::default().element(remove_poster_when_local),
                ),
            ],
            ..RewriteStrSettings::new()
        },
    )
    .map_err(|error| error.to_string())
}

/// Replace the whole element with plain-text when `attribute` is local media.
fn replace_when_local(element: &mut Element, attribute: &str, placeholder: &str) -> HandlerResult {
    let Some(value) = element.get_attribute(attribute) else {
        return Ok(());
    };
    if is_local_media_url(&value) {
        element.replace(placeholder, ContentType::Text);
    }
    Ok(())
}

/// Remove a `source` element whose `src` points at local dictionary media.
fn remove_when_local(element: &mut Element) -> HandlerResult {
    let Some(value) = element.get_attribute("src") else {
        return Ok(());
    };
    if is_local_media_url(&value) {
        element.remove();
    }
    Ok(())
}

/// Drop a `video[poster]` attribute pointing at local dictionary media.
fn remove_poster_when_local(element: &mut Element) -> HandlerResult {
    let Some(value) = element.get_attribute("poster") else {
        return Ok(());
    };
    if is_local_media_url(&value) {
        element.remove_attribute("poster");
    }
    Ok(())
}

/// Whether a media URL refers to a local dictionary resource.
fn is_local_media_url(value: &str) -> bool {
    let value = value.trim();
    prefix_matches(value, MDX_RES_SCHEME)
        || prefix_matches(value, &format!("http://{MDX_RES_WINDOWS_HOST}"))
        || prefix_matches(value, &format!("https://{MDX_RES_WINDOWS_HOST}"))
}

/// Case-insensitive ASCII prefix match that never slices inside a char.
fn prefix_matches(value: &str, prefix: &str) -> bool {
    value
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

/// Parse one static selector; every selector used here is a compile-time constant.
fn parse_selector(raw: &str) -> Option<Selector> {
    match raw.parse::<Selector>() {
        Ok(selector) => Some(selector),
        Err(error) => {
            tracing::warn!(selector = raw, error = %error, "invalid media selector");
            None
        }
    }
}
