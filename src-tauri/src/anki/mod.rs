//! AnkiConnect 客户端与字段映射。契约见 docs/BACKEND_STRUCTURE.md 6。

pub mod client;
pub mod mapping;

pub use client::{AddNoteRequest, AnkiClient};
pub use mapping::{
    build_fields, build_note_request, escape_html, html_with_breaks, validate_field_map,
    MAX_DEFINITION_BYTES, UNKNOWN_SOURCE,
};
