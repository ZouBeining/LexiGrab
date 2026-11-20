// SPDX-License-Identifier: AGPL-3.0-only
//! Context fingerprints for the `saved_cards` duplicate-context check.
//!
//! A fingerprint is a UUID v5 over normalized text: NFKC, lowercase, collapse
//! whitespace and trim (docs/BACKEND_STRUCTURE.md §3). Sentence and definition
//! keys share one fixed namespace but use different domain prefixes, so the
//! same text stored in a different column cannot collide.
//!
//! Definition keys are computed from the plain text of an HTML fragment; tags
//! are stripped with `scraper` and HTML entities are decoded by the parser.

use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

/// Fixed namespace for every saved-card context fingerprint.
///
/// Changing this value invalidates all existing fingerprints (every saved card
/// looks unseen again), so it must stay stable across releases.
pub const CONTEXT_NAMESPACE: Uuid = Uuid::from_u128(0x5f2b_9d1c_4a7e_4b1d_9c6a_08f3_2e7d_51a4);

/// Domain prefix for `saved_cards.sentence_key`.
const SENTENCE_DOMAIN: &str = "sentence";
/// Domain prefix for `saved_cards.definition_key`.
const DEFINITION_DOMAIN: &str = "definition";

/// Normalize a text key: NFKC, lowercase, collapse whitespace, trim.
///
/// Used for the `word` column and as the input of every UUID v5 fingerprint.
pub fn normalize_key(text: &str) -> String {
    let normalized: String = text.nfkc().collect::<String>().to_lowercase();
    collapse_whitespace(&normalized)
}

/// UUID v5 key for a sentence; missing sentences map to the empty string.
pub fn sentence_key(sentence: &str) -> String {
    uuid_key(SENTENCE_DOMAIN, sentence)
}

/// Plain text of a definition given as HTML (or plain text).
///
/// Tags are stripped with `scraper`, HTML entities are decoded by the parser,
/// then whitespace runs are collapsed and the result trimmed. Case is
/// preserved; [`definition_key`] lowercases it afterwards. This shared helper
/// keeps the UI's context check and the stored fingerprint in sync.
pub fn definition_text(html: &str) -> String {
    let document = scraper::Html::parse_document(html);
    let text: String = document.root_element().text().collect();
    collapse_whitespace(&text)
}

/// UUID v5 key for a definition given as HTML (or plain text); tags are
/// stripped and entities decoded before normalization.
pub fn definition_key(html: &str) -> String {
    definition_key_from_text(&definition_text(html))
}

/// UUID v5 key for a definition that is already plain text.
///
/// [`crate::db::CardStore`] uses this for its `definition_text` argument: the
/// caller runs [`definition_text`] on the HTML first, so plain text is never
/// parsed as HTML a second time (escaped `<`/`&` would otherwise be lost).
pub(crate) fn definition_key_from_text(text: &str) -> String {
    uuid_key(DEFINITION_DOMAIN, text)
}

/// Collapse every whitespace run into one space and trim the edges.
fn collapse_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            // Only emit a single space between visible characters; leading and
            // trailing whitespace is dropped by never flushing at the edges.
            if !result.is_empty() {
                pending_space = true;
            }
        } else {
            if pending_space {
                result.push(' ');
                pending_space = false;
            }
            result.push(character);
        }
    }
    result
}

/// Build a stable UUID v5 string from a domain prefix and normalized text.
fn uuid_key(domain: &str, text: &str) -> String {
    let normalized = normalize_key(text);
    Uuid::new_v5(
        &CONTEXT_NAMESPACE,
        format!("{domain}:{normalized}").as_bytes(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_applies_nfkc_and_lowercase() {
        // Full-width characters normalize to ASCII; case folds too.
        assert_eq!(normalize_key("Ｈｅｌｌｏ"), "hello");
        assert_eq!(normalize_key("Straße"), "straße");
    }

    #[test]
    fn normalize_key_collapses_and_trims_whitespace() {
        assert_eq!(
            normalize_key("  hello \t world \r\n next  "),
            "hello world next"
        );
        assert_eq!(normalize_key(" \t\n "), "");
        assert_eq!(normalize_key("中文　空格"), "中文 空格");
    }

    #[test]
    fn sentence_key_is_stable_across_normalization() {
        assert_eq!(sentence_key("Hello   World"), sentence_key(" hello world "));
        assert_eq!(sentence_key(""), sentence_key("   "));
        assert_ne!(sentence_key("a b"), sentence_key("ab"));
    }

    #[test]
    fn definition_key_strips_tags_and_decodes_entities() {
        let html = "<div><b>Hello</b> &amp; <i>bye</i></div>";
        assert_eq!(definition_key(html), definition_key("Hello & bye"));
        assert_eq!(definition_text(html), "Hello & bye");
    }

    #[test]
    fn definition_text_preserves_case_and_collapses_whitespace() {
        assert_eq!(
            definition_text("<p>  Hello\n\n  World  </p>"),
            "Hello World"
        );
        assert_eq!(definition_text(""), "");
    }

    #[test]
    fn fingerprint_is_a_lowercase_uuid() {
        let key = sentence_key("hello");
        assert_eq!(key.len(), 36);
        assert_eq!(key, key.to_lowercase());
        assert_eq!(key.matches('-').count(), 4);
    }

    #[test]
    fn sentence_and_definition_domains_do_not_collide() {
        assert_ne!(sentence_key("hello"), definition_key("hello"));
    }
}
