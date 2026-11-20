use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 词典条目。契约见 docs/BACKEND_STRUCTURE.md 2.1。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub headword: String,
    pub definition_html: String,
    pub phonetic: Option<String>,
    pub resources: Vec<ResourceRef>,
    /// 解析器返回空串，由数据源层统一填写。
    pub source_name: String,
}

impl Entry {
    pub fn new(headword: impl Into<String>, definition_html: impl Into<String>) -> Self {
        Self {
            headword: headword.into(),
            definition_html: definition_html.into(),
            phonetic: None,
            resources: Vec::new(),
            source_name: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceRef {
    Local { dict_id: String, path: String },
    Remote { url: String },
}

/// 数据源配置。契约见 docs/BACKEND_STRUCTURE.md 2.2。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub kind: SourceKind,
    pub enabled: bool,
    pub priority: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    LocalFile {
        paths: Vec<PathBuf>,
    },
    RemoteHttp {
        url_template: String,
        response_parser: ResponseParserConfig,
        auth_header: Option<String>,
        auth_value: Option<String>,
        timeout_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ResponseParserConfig {
    Jsonpath {
        expression: String,
    },
    Script {
        path: PathBuf,
        timeout_ms: Option<u64>,
    },
}

impl SourceConfig {
    /// 生成后端 id：`local-<uuid8>` / `remote-<uuid8>`。
    pub fn generate_id(kind: &SourceKind) -> String {
        let prefix = match kind {
            SourceKind::LocalFile { .. } => "local",
            SourceKind::RemoteHttp { .. } => "remote",
        };
        let uuid = uuid::Uuid::new_v4().simple().to_string();
        format!("{prefix}-{}", &uuid[..8])
    }
}

/// 写入 Anki 前的数据。契约见 docs/BACKEND_STRUCTURE.md 2.4。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnkiPayload {
    pub word: String,
    pub definition_html: String,
    pub sentence: Option<String>,
    pub source: String,
    /// 追加标签；AI 释义卡带 `ai-generated`，缺省为空。
    #[serde(default)]
    pub tags: Vec<String>,
}

impl AnkiPayload {
    pub fn new(
        word: impl Into<String>,
        definition_html: impl Into<String>,
        sentence: Option<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            word: word.into(),
            definition_html: definition_html.into(),
            sentence,
            source: source.into(),
            tags: Vec::new(),
        }
    }

    /// 追加标签（AI 卡用），保持调用方顺序。
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_round_trips_through_json() {
        let entry = Entry {
            headword: "hello".to_string(),
            definition_html: "<b>hi</b>".to_string(),
            phonetic: Some("/həˈləʊ/".to_string()),
            resources: vec![
                ResourceRef::Local {
                    dict_id: "local-1".to_string(),
                    path: "img/a.png".to_string(),
                },
                ResourceRef::Remote {
                    url: "https://example.com/a.png".to_string(),
                },
            ],
            source_name: "Mini".to_string(),
        };
        let json = serde_json::to_string(&entry).expect("serialize entry");
        let decoded: Entry = serde_json::from_str(&json).expect("deserialize entry");
        assert_eq!(decoded, entry);
        assert!(json.contains("\"kind\":\"local\""));
    }

    #[test]
    fn source_config_deserializes_toml_example() {
        let toml = r#"
            id = "local-00000001"
            name = "Mini"
            type = "local_file"
            enabled = true
            priority = 10
            paths = ["tests/fixtures/mini.mdx"]
        "#;
        let config: SourceConfig = toml::from_str(toml).expect("deserialize source config");
        assert_eq!(config.id, "local-00000001");
        assert_eq!(config.name, "Mini");
        assert!(config.enabled);
        assert_eq!(config.priority, 10);
        match config.kind {
            SourceKind::LocalFile { paths } => {
                assert_eq!(paths, vec![PathBuf::from("tests/fixtures/mini.mdx")]);
            }
            SourceKind::RemoteHttp { .. } => panic!("expected local_file"),
        }
    }

    #[test]
    fn generated_id_has_expected_prefix() {
        let id = SourceConfig::generate_id(&SourceKind::LocalFile { paths: Vec::new() });
        assert!(id.starts_with("local-"));
        assert_eq!(id.len(), "local-".len() + 8);
    }
}
