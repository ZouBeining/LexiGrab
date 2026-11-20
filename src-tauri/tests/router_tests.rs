// SPDX-License-Identifier: AGPL-3.0-only
//! 本地查询路由集成测试：真实 MDX 夹具 + 多源排序 + 失败隔离。

use std::path::{Path, PathBuf};

use lexigrab_lib::core::config::{ConfigStore, QueryStrategy, Settings};
use lexigrab_lib::core::entry::{SourceConfig, SourceKind};
use lexigrab_lib::core::query_router::QueryRouter;
use lexigrab_lib::core::registry::SourceRegistry;
use lexigrab_lib::core::runtime::RecordingSink;
use lexigrab_lib::sources::SourceStatus;

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_settings() -> Settings {
    let store = ConfigStore::with_config_path(fixture_dir().join("mini-config.toml"));
    let load = store.load();
    assert!(load.warning.is_none(), "warning: {:?}", load.warning);
    load.settings
}

fn local_source(id: &str, priority: i32, paths: &[&str]) -> SourceConfig {
    SourceConfig {
        id: id.to_string(),
        name: format!("Source {id}"),
        enabled: true,
        priority,
        kind: SourceKind::LocalFile {
            paths: paths.iter().map(PathBuf::from).collect(),
        },
    }
}

#[test]
fn fixture_source_is_ready_and_returns_source_name() {
    let settings = fixture_settings();
    let registry = SourceRegistry::from_settings(&settings, &fixture_dir());
    assert_eq!(registry.len(), 1);
    let states = registry.states();
    assert_eq!(states.len(), 1);
    assert_eq!(states[0].id, "local-mini");
    assert_eq!(states[0].status, SourceStatus::Ready { entry_count: 3 });

    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router.lookup("hello", None, &sink).expect("lookup");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| entry.source_name == "Mini"));
    assert!(sink.events().is_empty());
}

#[test]
fn disabled_source_does_not_participate() {
    let mut settings = fixture_settings();
    settings.sources[0].enabled = false;
    let registry = SourceRegistry::from_settings(&settings, &fixture_dir());
    assert!(registry.is_empty());
    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router.lookup("hello", None, &sink).expect("lookup");
    assert!(entries.is_empty());
}

#[test]
fn source_order_follows_priority_then_id() {
    let settings = fixture_settings();
    let mut registry = SourceRegistry::empty();
    for (id, priority) in [("b", 20), ("a", 10)] {
        let config = local_source(
            id,
            priority,
            &[Path::new("mini.mdx").to_str().unwrap_or("mini.mdx")],
        );
        let source = lexigrab_lib::sources::local_file::LocalFileSource::open(
            config.clone(),
            &fixture_dir(),
        );
        registry.register(config, std::sync::Arc::new(source));
    }
    let ids: Vec<String> = registry
        .states()
        .into_iter()
        .map(|state| state.id)
        .collect();
    assert_eq!(ids, vec!["a", "b"]);

    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router.lookup("hello", None, &sink).expect("lookup");
    assert!(entries.len() >= 2);
    assert_eq!(entries[0].source_name, "Source a");
}

#[test]
fn first_hit_returns_only_first_source() {
    let mut settings = fixture_settings();
    settings.general.query_strategy = QueryStrategy::FirstHit;
    let mut registry = SourceRegistry::empty();
    for (id, priority) in [("a", 10), ("b", 20)] {
        let config = local_source(id, priority, &["mini.mdx"]);
        let source = lexigrab_lib::sources::local_file::LocalFileSource::open(
            config.clone(),
            &fixture_dir(),
        );
        registry.register(config, std::sync::Arc::new(source));
    }
    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router.lookup("hello", None, &sink).expect("lookup");
    assert!(!entries.is_empty());
    assert!(entries.iter().all(|entry| entry.source_name == "Source a"));
}

#[test]
fn broken_file_does_not_break_same_source_other_files() {
    let mut settings = fixture_settings();
    settings.sources = vec![local_source("mixed", 10, &["broken.mdx", "mini.mdx"])];
    let registry = SourceRegistry::from_settings(&settings, &fixture_dir());
    assert_eq!(registry.len(), 1);
    let states = registry.states();
    assert!(
        states[0].status.is_ready(),
        "status: {:?}",
        states[0].status
    );

    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router.lookup("hello", None, &sink).expect("lookup");
    assert!(!entries.is_empty());
    assert!(entries
        .iter()
        .all(|entry| entry.source_name == "Source mixed"));
}

#[test]
fn all_sources_failing_returns_error() {
    let mut settings = fixture_settings();
    settings.sources = vec![local_source("bad", 10, &["broken.mdx"])];
    let registry = SourceRegistry::from_settings(&settings, &fixture_dir());
    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let error = router.lookup("hello", None, &sink).expect_err("error");
    assert_eq!(error.code, "DICT_NOT_READY");
    assert_eq!(sink.failures().len(), 1);
}

#[test]
fn missing_word_returns_empty_result_not_error() {
    let settings = fixture_settings();
    let registry = SourceRegistry::from_settings(&settings, &fixture_dir());
    let router = QueryRouter::new(&registry, &settings);
    let sink = RecordingSink::new();
    let entries = router
        .lookup("definitely-not-in-dictionary", None, &sink)
        .expect("lookup");
    assert!(entries.is_empty());
}
