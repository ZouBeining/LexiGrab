use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::core::entry::SourceConfig;
use crate::core::error::ConfigError;

/// 当前配置 schema 版本。
pub const SCHEMA_VERSION: u32 = 2;

/// 应用目录解析。测试可用 `LEXIGRAB_HOME` 覆盖，避免写到真实用户目录。
pub struct Paths {
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        if let Some(config_dir) = std::env::var_os("LEXIGRAB_CONFIG_DIR") {
            let config_dir = PathBuf::from(config_dir);
            let log_dir = std::env::var_os("LEXIGRAB_HOME")
                .map(PathBuf::from)
                .map(|base| base.join("logs"))
                .unwrap_or_else(|| config_dir.join("logs"));
            return Self {
                config_dir,
                log_dir,
            };
        }
        if let Some(root) = std::env::var_os("LEXIGRAB_HOME") {
            let base = PathBuf::from(root);
            return Self {
                config_dir: base.join("config"),
                log_dir: base.join("logs"),
            };
        }
        let config_dir = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LexiGrab");
        let log_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LexiGrab")
            .join("logs");
        Self {
            config_dir,
            log_dir,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        ensure_dir(&self.config_dir)?;
        ensure_dir(&self.log_dir)?;
        Ok(())
    }
}

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub meta: MetaSettings,
    pub general: GeneralSettings,
    pub anki: AnkiSettings,
    pub bridge: BridgeSettings,
    pub cache: CacheSettings,
    pub sense: SenseSettings,
    pub llm: LlmSettings,
    pub sources: Vec<SourceConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MetaSettings {
    pub schema_version: u32,
    pub device_id: String,
}

impl Default for MetaSettings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            device_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub hotkey: String,
    pub theme: Theme,
    pub query_strategy: QueryStrategy,
    pub max_entries_per_source: u32,
    pub max_entries_total: u32,
    pub start_minimized: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            hotkey: "CmdOrCtrl+Shift+D".to_string(),
            theme: Theme::Figma,
            query_strategy: QueryStrategy::LocalFirst,
            max_entries_per_source: 3,
            max_entries_total: 20,
            start_minimized: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Figma,
    Apple,
    Cursor,
    /// PROMPT 示例中的历史值；0.1.0 落到默认主题。
    System,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStrategy {
    #[default]
    LocalFirst,
    FirstHit,
    Aggregate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnkiSettings {
    pub endpoint: String,
    pub deck: String,
    pub model: String,
    pub tags: Vec<String>,
    pub duplicate_policy: DuplicatePolicy,
    pub field_map: AnkiFieldMap,
}

impl Default for AnkiSettings {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8765".to_string(),
            deck: "Default".to_string(),
            model: "Basic".to_string(),
            tags: vec!["global-dict".to_string()],
            duplicate_policy: DuplicatePolicy::Warn,
            field_map: AnkiFieldMap::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    #[default]
    Warn,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AnkiFieldMap {
    pub word: String,
    pub definition: String,
    pub sentence: String,
    pub source: String,
}

impl Default for AnkiFieldMap {
    fn default() -> Self {
        Self {
            word: "Word".to_string(),
            definition: "Definition".to_string(),
            sentence: "Sentence".to_string(),
            source: "Source".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BridgeSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
}

impl Default for BridgeSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 12787,
            token: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheSettings {
    pub ttl_hours: u32,
    pub negative_ttl_minutes: u32,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            ttl_hours: 24,
            negative_ttl_minutes: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SenseSettings {
    pub enabled: bool,
    pub selector_endpoint: String,
    pub selector_timeout_ms: u64,
    pub selector_allow_remote: bool,
    pub low_confidence: f32,
    pub high_confidence: f32,
    pub max_senses: u32,
}

impl Default for SenseSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            selector_endpoint: "http://127.0.0.1:8787/v1/select".to_string(),
            selector_timeout_ms: 800,
            selector_allow_remote: false,
            low_confidence: 0.55,
            high_confidence: 0.72,
            max_senses: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub timeout_ms: u64,
    pub max_tokens: u32,
    pub allow_remote: bool,
    pub cache_ttl_hours: u32,
}

impl Default for LlmSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://127.0.0.1:11434/v1/chat/completions".to_string(),
            model: String::new(),
            api_key_env: "LEXIGRAB_LLM_API_KEY".to_string(),
            timeout_ms: 15000,
            max_tokens: 320,
            allow_remote: false,
            cache_ttl_hours: 168,
        }
    }
}

/// 生成 32 字节 hex 令牌（64 个小写十六进制字符）。
pub fn generate_bridge_token() -> String {
    let first = uuid::Uuid::new_v4().simple().to_string();
    let second = uuid::Uuid::new_v4().simple().to_string();
    format!("{first}{second}")
}

/// 生成设备 id。
pub fn generate_device_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl Settings {
    /// 补齐首次启动需要生成的字段。返回是否有改动。
    pub fn normalize(&mut self) -> bool {
        let mut changed = false;
        if self.meta.schema_version != SCHEMA_VERSION {
            self.meta.schema_version = SCHEMA_VERSION;
            changed = true;
        }
        if self.meta.device_id.trim().is_empty() {
            self.meta.device_id = generate_device_id();
            changed = true;
        }
        if self.bridge.token.trim().is_empty() {
            self.bridge.token = generate_bridge_token();
            changed = true;
        }
        changed
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let general = &self.general;
        if general.hotkey.trim().is_empty() {
            return Err(invalid("general.hotkey 不能为空"));
        }
        let parts: Vec<&str> = general.hotkey.split('+').map(|part| part.trim()).collect();
        let has_modifier = parts.iter().any(|part| {
            matches!(
                part.to_ascii_lowercase().as_str(),
                "cmdorctrl" | "cmd" | "ctrl" | "alt" | "shift" | "super" | "command" | "control"
            )
        });
        if !has_modifier || parts.len() < 2 {
            return Err(invalid("general.hotkey 必须包含修饰键"));
        }
        if !(1..=10).contains(&general.max_entries_per_source) {
            return Err(invalid("general.max_entries_per_source 范围是 1–10"));
        }
        if !(5..=50).contains(&general.max_entries_total) {
            return Err(invalid("general.max_entries_total 范围是 5–50"));
        }

        validate_http_endpoint("anki.endpoint", &self.anki.endpoint)?;
        if self.anki.deck.trim().is_empty() {
            return Err(invalid("anki.deck 不能为空"));
        }
        if self.anki.model.trim().is_empty() {
            return Err(invalid("anki.model 不能为空"));
        }
        for (name, value) in [
            ("word", &self.anki.field_map.word),
            ("definition", &self.anki.field_map.definition),
            ("sentence", &self.anki.field_map.sentence),
            ("source", &self.anki.field_map.source),
        ] {
            if value.trim().is_empty() {
                return Err(invalid(format!("anki.field_map.{name} 不能为空")));
            }
        }

        if !(1024..=65535).contains(&self.bridge.port) {
            return Err(invalid("bridge.port 范围是 1024–65535"));
        }
        if !self.bridge.token.is_empty() {
            let valid = self.bridge.token.len() == 64
                && self
                    .bridge
                    .token
                    .chars()
                    .all(|character| character.is_ascii_hexdigit());
            if !valid {
                return Err(invalid("bridge.token 必须是 64 位十六进制字符串"));
            }
        }

        if !(1..=720).contains(&self.cache.ttl_hours) {
            return Err(invalid("cache.ttl_hours 范围是 1–720"));
        }
        if !(1..=60).contains(&self.cache.negative_ttl_minutes) {
            return Err(invalid("cache.negative_ttl_minutes 范围是 1–60"));
        }

        validate_http_endpoint("sense.selector_endpoint", &self.sense.selector_endpoint)?;
        if !(200..=3000).contains(&self.sense.selector_timeout_ms) {
            return Err(invalid("sense.selector_timeout_ms 范围是 200–3000"));
        }
        if !(self.sense.low_confidence.is_finite() && self.sense.low_confidence > 0.0) {
            return Err(invalid("sense.low_confidence 必须大于 0"));
        }
        if !(self.sense.high_confidence.is_finite()
            && self.sense.high_confidence <= 1.0
            && self.sense.high_confidence > self.sense.low_confidence)
        {
            return Err(invalid(
                "sense.high_confidence 必须大于 low_confidence 且不超过 1",
            ));
        }
        if !(1..=20).contains(&self.sense.max_senses) {
            return Err(invalid("sense.max_senses 范围是 1–20"));
        }

        validate_http_endpoint("llm.endpoint", &self.llm.endpoint)?;
        if !(1000..=60_000).contains(&self.llm.timeout_ms) {
            return Err(invalid("llm.timeout_ms 范围是 1000–60000"));
        }
        if !(1..=4096).contains(&self.llm.max_tokens) {
            return Err(invalid("llm.max_tokens 范围是 1–4096"));
        }
        if !(1..=720).contains(&self.llm.cache_ttl_hours) {
            return Err(invalid("llm.cache_ttl_hours 范围是 1–720"));
        }
        if self.llm.enabled && self.llm.model.trim().is_empty() {
            return Err(invalid("启用 LLM 时 llm.model 不能为空"));
        }

        self.validate_sources()
    }

    fn validate_sources(&self) -> Result<(), ConfigError> {
        let mut ids: Vec<&str> = Vec::with_capacity(self.sources.len());
        let mut paths: Vec<PathBuf> = Vec::new();
        for (index, source) in self.sources.iter().enumerate() {
            if source.id.trim().is_empty() {
                return Err(invalid(format!("sources[{index}].id 不能为空")));
            }
            if ids.contains(&source.id.as_str()) {
                return Err(invalid(format!("数据源 id 重复: {}", source.id)));
            }
            ids.push(source.id.as_str());
            if source.name.trim().is_empty() {
                return Err(invalid(format!("数据源 {} 的 name 不能为空", source.id)));
            }
            match &source.kind {
                crate::core::entry::SourceKind::LocalFile { paths: local } => {
                    if local.is_empty() {
                        return Err(invalid(format!("数据源 {} 未配置 paths", source.id)));
                    }
                    for path in local {
                        validate_local_path(&source.id, path)?;
                        if paths.contains(path) {
                            return Err(invalid(format!(
                                "同一路径不得出现在多个源中: {}",
                                path.display()
                            )));
                        }
                        paths.push(path.clone());
                    }
                }
                crate::core::entry::SourceKind::RemoteHttp {
                    url_template,
                    response_parser,
                    timeout_ms,
                    ..
                } => {
                    let occurrences = url_template.matches("{word}").count();
                    if occurrences != 1 {
                        return Err(invalid(format!(
                            "数据源 {} 的 url_template 必须恰好包含一个 {{word}}",
                            source.id
                        )));
                    }
                    validate_http_endpoint(
                        &format!("数据源 {} 的 url_template", source.id),
                        &url_template.replace("{word}", "test"),
                    )?;
                    if !(200..=15_000).contains(timeout_ms) {
                        return Err(invalid(format!(
                            "数据源 {} 的 timeout_ms 范围是 200–15000",
                            source.id
                        )));
                    }
                    if let crate::core::entry::ResponseParserConfig::Script {
                        timeout_ms: Some(script_timeout),
                        ..
                    } = response_parser
                    {
                        if !(50..=1000).contains(script_timeout) {
                            return Err(invalid(format!(
                                "数据源 {} 的脚本 timeout_ms 范围是 50–1000",
                                source.id
                            )));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 把配置中的本地路径按配置文件目录解析为绝对路径。
    pub fn resolve_local_path(&self, base_dir: &Path, path: &Path) -> PathBuf {
        let _ = self;
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            base_dir.join(path)
        }
    }
}

fn invalid(message: impl Into<String>) -> ConfigError {
    ConfigError::Validation(message.into())
}

fn validate_http_endpoint(label: &str, value: &str) -> Result<(), ConfigError> {
    let parsed = url::Url::parse(value)
        .map_err(|error| invalid(format!("{label} 不是合法 URL: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(invalid(format!("{label} 只允许 http/https")));
    }
    if parsed.host_str().is_none() {
        return Err(invalid(format!("{label} 缺少主机名")));
    }
    Ok(())
}

fn validate_local_path(source_id: &str, path: &Path) -> Result<(), ConfigError> {
    if path.as_os_str().is_empty() {
        return Err(invalid(format!("数据源 {source_id} 含空路径")));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            return Err(invalid(format!(
                "数据源 {source_id} 的路径不得包含 ..: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

/// 配置加载结果。损坏文件会备份并回退默认值。
#[derive(Debug, Clone)]
pub struct ConfigLoad {
    pub settings: Settings,
    pub config_path: PathBuf,
    pub created: bool,
    pub backup: Option<PathBuf>,
    pub warning: Option<String>,
}

/// 配置文件读写。契约见 docs/BACKEND_STRUCTURE.md 2.3。
pub struct ConfigStore {
    config_path: PathBuf,
}

impl ConfigStore {
    /// 使用目录下的 `config.toml`（桌面端默认布局）。
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_dir.into().join("config.toml"),
        }
    }

    /// 使用指定的配置文件路径（开发 CLI 的 `--config`）。
    pub fn with_config_path(config_path: impl Into<PathBuf>) -> Self {
        Self {
            config_path: config_path.into(),
        }
    }

    pub fn from_paths(paths: &Paths) -> Self {
        Self::new(paths.config_dir.clone())
    }

    pub fn config_dir(&self) -> &Path {
        self.config_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_path.clone()
    }

    pub fn load(&self) -> ConfigLoad {
        let config_path = self.config_path();
        if !config_path.exists() {
            let mut settings = Settings::default();
            settings.normalize();
            let warning = match self.save(&settings) {
                Ok(()) => None,
                Err(error) => Some(format!("写入默认配置失败: {error}")),
            };
            return ConfigLoad {
                settings,
                config_path,
                created: true,
                backup: None,
                warning,
            };
        }

        let raw = match std::fs::read_to_string(&config_path) {
            Ok(raw) => raw,
            Err(error) => {
                return self.recover(config_path, format!("读取配置失败: {error}"), None);
            }
        };

        let mut value: toml::Value = match toml::from_str(&raw) {
            Ok(value) => value,
            Err(error) => {
                return self.recover(config_path, format!("配置解析失败: {error}"), Some(raw));
            }
        };
        let migrated = migrate_config(&mut value);

        let mut settings: Settings = match value.try_into() {
            Ok(settings) => settings,
            Err(error) => {
                return self.recover(config_path, format!("配置字段不合法: {error}"), Some(raw));
            }
        };
        if let Err(error) = settings.validate() {
            return self.recover(config_path, error.to_string(), Some(raw));
        }
        settings.normalize();
        let warning = if migrated {
            Some("配置已从 schema_version 1 迁移到 2，建议保存一次".to_string())
        } else {
            None
        };
        ConfigLoad {
            settings,
            config_path,
            created: false,
            backup: None,
            warning,
        }
    }

    fn recover(&self, config_path: PathBuf, reason: String, raw: Option<String>) -> ConfigLoad {
        let backup = backup_path(&config_path);
        let mut backup_result: Option<String> = None;
        if let Some(raw) = raw {
            match write_new_file(&backup, raw.as_bytes()) {
                Ok(()) => {
                    if let Err(error) = std::fs::remove_file(&config_path) {
                        backup_result = Some(error.to_string());
                    }
                }
                Err(error) => backup_result = Some(error.to_string()),
            }
        } else if let Err(error) = std::fs::rename(&config_path, &backup) {
            backup_result = Some(error.to_string());
        }

        let mut settings = Settings::default();
        settings.normalize();
        let mut messages = vec![format!("{reason}，已使用默认配置")];
        match backup_result {
            Some(error) => messages.push(format!("备份配置失败: {error}")),
            None if backup.exists() => {
                messages.push(format!("原配置已备份到 {}", backup.display()))
            }
            None => {}
        }
        ConfigLoad {
            settings,
            config_path,
            created: false,
            backup: Some(backup),
            warning: Some(messages.join("；")),
        }
    }

    /// 校验通过后原子保存；失败不写盘。
    pub fn save(&self, settings: &Settings) -> Result<(), ConfigError> {
        settings.validate()?;
        let serialized = toml::to_string_pretty(settings)
            .map_err(|error| ConfigError::Save(error.to_string()))?;
        ensure_dir(self.config_dir()).map_err(|error| ConfigError::Save(error.to_string()))?;
        let target = self.config_path();
        let tmp = target.with_extension("toml.tmp");
        write_new_file(&tmp, serialized.as_bytes())
            .map_err(|error| ConfigError::Save(error.to_string()))?;
        replace_file(&tmp, &target).map_err(|error| ConfigError::Save(error.to_string()))
    }
}

fn write_new_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn replace_file(tmp: &Path, target: &Path) -> std::io::Result<()> {
    // Windows 上 rename 不能覆盖已存在文件；契约要求先删除旧文件再替换。
    if target.exists() {
        std::fs::remove_file(target)?;
    }
    match std::fs::rename(tmp, target) {
        Ok(()) => Ok(()),
        Err(error) => {
            // 删除后替换失败时保留 tmp，便于人工恢复。
            if tmp.exists() && !target.exists() {
                let _ = std::fs::copy(tmp, target);
            }
            Err(error)
        }
    }
}

fn backup_path(config_path: &Path) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let file_name = config_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "config.toml".to_string());
    config_path.with_file_name(format!("{file_name}.bak-{stamp}"))
}

/// schema_version 1 -> 2：`anki.allow_duplicate` 迁移为 `duplicate_policy`。
fn migrate_config(value: &mut toml::Value) -> bool {
    let schema_version = value
        .get("meta")
        .and_then(|meta| meta.get("schema_version"))
        .and_then(toml::Value::as_integer)
        .unwrap_or(2);
    if schema_version >= 2 {
        return false;
    }
    if let Some(anki) = value.get_mut("anki").and_then(toml::Value::as_table_mut) {
        let allow_duplicate = anki
            .remove("allow_duplicate")
            .and_then(|value| value.as_bool());
        if let Some(allow) = allow_duplicate {
            let policy = if allow { "allow" } else { "warn" };
            anki.entry("duplicate_policy".to_string())
                .or_insert_with(|| toml::Value::String(policy.to_string()));
        }
    }
    if let Some(meta) = value.get_mut("meta").and_then(toml::Value::as_table_mut) {
        meta.insert(
            "schema_version".to_string(),
            toml::Value::Integer(i64::from(SCHEMA_VERSION)),
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> ConfigStore {
        // 测试不写 %TEMP%：本机 Defender 受控文件夹访问会拒绝该目录。
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-tmp")
            .join(tag);
        if root.exists() {
            let _ = std::fs::remove_dir_all(&root);
        }
        ConfigStore::new(root)
    }

    #[test]
    fn paths_resolve_follows_overrides() {
        // 环境变量是进程级的，放在同一个测试里避免并行竞争。
        let root = std::env::temp_dir().join("lexigrab-paths-test-home");
        std::env::set_var("LEXIGRAB_HOME", &root);
        std::env::remove_var("LEXIGRAB_CONFIG_DIR");
        let paths = Paths::resolve();
        assert_eq!(paths.config_dir, root.join("config"));
        assert_eq!(paths.log_dir, root.join("logs"));

        std::env::set_var("LEXIGRAB_CONFIG_DIR", &root);
        let paths = Paths::resolve();
        std::env::remove_var("LEXIGRAB_CONFIG_DIR");
        std::env::remove_var("LEXIGRAB_HOME");
        assert_eq!(paths.config_dir, root);
    }

    #[test]
    fn default_settings_are_valid_after_normalize() {
        let mut settings = Settings::default();
        assert!(settings.normalize());
        assert_eq!(settings.meta.schema_version, SCHEMA_VERSION);
        assert_eq!(settings.bridge.token.len(), 64);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn save_then_load_round_trips() {
        let store = temp_store("roundtrip");
        let load = store.load();
        assert!(load.created);
        assert!(load.warning.is_none());
        let mut settings = load.settings;
        settings.general.max_entries_per_source = 5;
        settings.anki.deck = "LexiGrab".to_string();
        store.save(&settings).expect("save config");
        assert!(store.config_path().exists());
        let reloaded = store.load();
        assert!(!reloaded.created);
        assert_eq!(reloaded.settings.general.max_entries_per_source, 5);
        assert_eq!(reloaded.settings.anki.deck, "LexiGrab");
        assert_eq!(reloaded.settings.bridge.token, settings.bridge.token);
    }

    #[test]
    fn prompt_toml_example_loads() {
        let store = temp_store("prompt");
        std::fs::create_dir_all(store.config_dir()).expect("create config dir");
        let example = r#"
[general]
hotkey = "CmdOrCtrl+Shift+D"
theme = "system"
query_strategy = "local_first"

[anki]
endpoint = "http://localhost:8765"
deck = "Default"
model = "Basic"
field_map = { word = "Word", definition = "Definition", sentence = "Sentence", source = "Source" }
duplicate_policy = "warn"

[sense]
enabled = false
selector_endpoint = "http://127.0.0.1:8787/v1/select"
selector_timeout_ms = 800
low_confidence = 0.55
high_confidence = 0.72

[llm]
enabled = false
endpoint = "http://127.0.0.1:11434/v1/chat/completions"
model = ""
api_key_env = "LEXIGRAB_LLM_API_KEY"
timeout_ms = 15000
allow_remote = false

[[sources]]
id = "local-cedict"
name = "CC-CEDICT"
type = "local_file"
enabled = true
priority = 10
paths = ["dictionaries/cedict.mdx"]

[[sources]]
id = "nas-dict"
name = "NAS 词典"
type = "remote_http"
enabled = false
priority = 20
url_template = "https://nas.local/dict/api/search?q={word}"
response_parser = { mode = "jsonpath", expression = "$.entries[*]" }
auth_header = "Authorization"
auth_value = "Bearer token"
timeout_ms = 3000
"#;
        std::fs::write(store.config_path(), example).expect("write config");
        let loaded = store.load();
        assert!(loaded.warning.is_none(), "warning: {:?}", loaded.warning);
        assert_eq!(loaded.settings.sources.len(), 2);
        assert_eq!(loaded.settings.general.theme, Theme::System);
        assert_eq!(loaded.settings.anki.field_map.word, "Word");
        assert_eq!(loaded.settings.bridge.token.len(), 64);
    }

    #[test]
    fn duplicate_source_id_is_rejected() {
        let store = temp_store("dup");
        let mut settings = store.load().settings;
        settings.sources = vec![
            source("local-1", "tests/fixtures/a.mdx"),
            source("local-1", "tests/fixtures/b.mdx"),
        ];
        let error = store.save(&settings).expect_err("save should fail");
        assert!(error.to_string().contains("重复"));
    }

    #[test]
    fn duplicate_path_across_sources_is_rejected() {
        let store = temp_store("duppath");
        let mut settings = store.load().settings;
        settings.sources = vec![
            source("local-1", "tests/fixtures/a.mdx"),
            source("local-2", "tests/fixtures/a.mdx"),
        ];
        let error = store.save(&settings).expect_err("save should fail");
        assert!(error.to_string().contains("同一路径"));
    }

    #[test]
    fn parent_dir_path_is_rejected() {
        let store = temp_store("parent");
        let mut settings = store.load().settings;
        settings.sources = vec![source("local-1", "../outside.mdx")];
        let error = store.save(&settings).expect_err("save should fail");
        assert!(error.to_string().contains(".."));
    }

    #[test]
    fn remote_template_without_word_is_rejected() {
        let store = temp_store("word");
        let mut settings = store.load().settings;
        settings.sources = vec![SourceConfig {
            id: "remote-1".to_string(),
            name: "Remote".to_string(),
            enabled: true,
            priority: 1,
            kind: crate::core::entry::SourceKind::RemoteHttp {
                url_template: "https://example.com/search".to_string(),
                response_parser: crate::core::entry::ResponseParserConfig::Jsonpath {
                    expression: "$.items[*]".to_string(),
                },
                auth_header: None,
                auth_value: None,
                timeout_ms: 3000,
            },
        }];
        let error = store.save(&settings).expect_err("save should fail");
        assert!(error.to_string().contains("{word}"));
    }

    #[test]
    fn confidence_thresholds_are_validated() {
        let mut settings = Settings::default();
        settings.normalize();
        settings.sense.low_confidence = 0.8;
        settings.sense.high_confidence = 0.7;
        assert!(settings.validate().is_err());
        settings.sense.low_confidence = 0.55;
        settings.sense.high_confidence = 1.0;
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn corrupt_config_is_backed_up_and_defaults_returned() {
        let store = temp_store("corrupt");
        std::fs::create_dir_all(store.config_dir()).expect("create config dir");
        std::fs::write(store.config_path(), "this is not toml = = =").expect("write config");
        let loaded = store.load();
        assert!(loaded.warning.is_some());
        assert!(loaded.backup.as_ref().expect("backup").exists());
        assert!(!store.config_path().exists());
        assert_eq!(loaded.settings.meta.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn schema_v1_allow_duplicate_migrates() {
        let store = temp_store("migrate");
        std::fs::create_dir_all(store.config_dir()).expect("create config dir");
        let legacy = r#"
[meta]
schema_version = 1

[anki]
allow_duplicate = true
"#;
        std::fs::write(store.config_path(), legacy).expect("write config");
        let loaded = store.load();
        assert_eq!(
            loaded.settings.anki.duplicate_policy,
            DuplicatePolicy::Allow
        );
        assert!(loaded
            .warning
            .as_deref()
            .unwrap_or_default()
            .contains("迁移"));
    }

    #[test]
    fn failed_save_keeps_previous_file_intact() {
        let store = temp_store("atomic");
        let load = store.load();
        let mut settings = load.settings;
        store.save(&settings).expect("save valid");
        let before = std::fs::read_to_string(store.config_path()).expect("read config");
        settings.bridge.token = "not-a-token".to_string();
        assert!(store.save(&settings).is_err());
        let after = std::fs::read_to_string(store.config_path()).expect("read config");
        assert_eq!(before, after);
        assert!(!store.config_path().with_extension("toml.tmp").exists());
    }

    fn source(id: &str, path: &str) -> SourceConfig {
        SourceConfig {
            id: id.to_string(),
            name: id.to_string(),
            enabled: true,
            priority: 10,
            kind: crate::core::entry::SourceKind::LocalFile {
                paths: vec![PathBuf::from(path)],
            },
        }
    }
}
