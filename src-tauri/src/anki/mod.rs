// SPDX-License-Identifier: AGPL-3.0-only
//! AnkiConnect client, note field mapping and the export pipeline.
//! Contract: docs/BACKEND_STRUCTURE.md §6.

pub mod client;
pub mod export;
pub mod mapping;

pub use client::{AddNoteRequest, AnkiClient};
pub use export::{
    context_definition_text, definition_text, export_to_anki, strip_local_media, ExportDeps,
    ExportOutcome,
};
pub use mapping::{
    build_fields, build_note_request, escape_html, html_with_breaks, validate_field_map,
    MAX_DEFINITION_BYTES, UNKNOWN_SOURCE,
};
