//! 配置加载/保存集成测试：使用真实文件与工作区内的临时目录。

use std::path::{Path, PathBuf};

use lexigrab_lib::core::config::{ConfigStore, Settings};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// 测试目录放在 `target/test-tmp`，避免本机 Defender 受控文件夹访问拦截 `%TEMP%`。
fn temp_store(tag: &str) -> ConfigStore {
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
fn fixture_config_loads_and_resolves_relative_paths() {
    let config_path = fixture_dir().join("mini-config.toml");
    let store = ConfigStore::with_config_path(config_path.clone());
    let load = store.load();
    assert!(!load.created, "夹具配置不应被重建");
    assert!(load.warning.is_none(), "warning: {:?}", load.warning);
    assert_eq!(load.settings.sources.len(), 1);
    assert_eq!(load.settings.sources[0].id, "local-mini");
    assert_eq!(
        load.settings.general.query_strategy,
        lexigrab_lib::core::config::QueryStrategy::LocalFirst
    );
    assert!(load.config_path.exists());
    assert_eq!(config_path, load.config_path);
}

#[test]
fn save_then_load_is_stable() {
    let store = temp_store("save-stable");
    let load = store.load();
    let mut settings = load.settings;
    settings.general.max_entries_per_source = 4;
    settings.general.max_entries_total = 30;
    settings.anki.deck = "LexiGrab Test".to_string();
    store.save(&settings).expect("save");
    let reloaded = store.load();
    assert_eq!(reloaded.settings.general.max_entries_per_source, 4);
    assert_eq!(reloaded.settings.general.max_entries_total, 30);
    assert_eq!(reloaded.settings.anki.deck, "LexiGrab Test");
    assert_eq!(reloaded.settings.bridge.token, settings.bridge.token);
    assert_eq!(reloaded.settings.meta.device_id, settings.meta.device_id);
}

#[test]
fn invalid_config_is_rejected_without_touching_disk() {
    let store = temp_store("invalid-no-write");
    let load = store.load();
    let before = std::fs::read_to_string(store.config_path()).expect("read config");
    let mut settings: Settings = load.settings;
    settings.sense.low_confidence = 0.9;
    settings.sense.high_confidence = 0.2;
    let error = store.save(&settings).expect_err("invalid thresholds");
    assert!(error.to_string().contains("high_confidence"));
    let after = std::fs::read_to_string(store.config_path()).expect("read config");
    assert_eq!(before, after);
}

#[test]
fn missing_config_is_created_with_device_id_and_token() {
    let store = temp_store("missing-created");
    let load = store.load();
    assert!(load.created);
    assert!(store.config_path().exists());
    let reloaded = store.load();
    assert!(!reloaded.created);
    assert_eq!(reloaded.settings.meta.schema_version, 2);
    assert!(!reloaded.settings.meta.device_id.is_empty());
    assert_eq!(reloaded.settings.bridge.token.len(), 64);
    assert_eq!(
        reloaded.settings.meta.device_id,
        load.settings.meta.device_id
    );
}

#[test]
fn invalid_config_file_is_backed_up() {
    let store = temp_store("invalid-backup");
    std::fs::create_dir_all(store.config_dir()).expect("create config dir");
    std::fs::write(store.config_path(), "[general]\nhotkey = \"\"\n").expect("write config");
    let load = store.load();
    assert!(load.warning.is_some());
    let backup = load.backup.expect("backup path");
    assert!(backup.exists());
    assert!(!store.config_path().exists());
}
