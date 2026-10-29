//! 本地查询路由：三种策略、失败隔离、条数上限与去重。
//! 保持同步签名，由调用方放到 blocking 线程（M2 用 spawn_blocking）。

use std::time::Instant;

use crate::core::config::{QueryStrategy, Settings};
use crate::core::entry::Entry;
use crate::core::error::IpcError;
use crate::core::registry::SourceRegistry;
use crate::core::runtime::{EventSink, QueryId, RuntimeEvent};
use crate::sources::DictionarySource;

/// 慢查询阈值：超过则发 SourceSlow 事件。
const SLOW_SOURCE_MS: u64 = 500;

pub struct QueryRouter<'a> {
    registry: &'a SourceRegistry,
    strategy: QueryStrategy,
    max_entries_per_source: usize,
    max_entries_total: usize,
}

impl<'a> QueryRouter<'a> {
    pub fn new(registry: &'a SourceRegistry, settings: &Settings) -> Self {
        Self {
            registry,
            strategy: settings.general.query_strategy,
            max_entries_per_source: settings.general.max_entries_per_source as usize,
            max_entries_total: settings.general.max_entries_total as usize,
        }
    }

    pub fn lookup(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        sink: &dyn EventSink,
    ) -> Result<Vec<Entry>, IpcError> {
        let query_id = QueryId::new();
        let sources = self.select_sources(source_ids)?;
        let mut entries: Vec<Entry> = Vec::new();
        let mut successes = 0usize;
        let mut failures = 0usize;
        let mut first_failure_code = "INTERNAL";
        let mut first_failure_message = "所有词典源查询失败".to_string();

        for source in sources {
            if entries.len() >= self.max_entries_total {
                break;
            }
            let started = Instant::now();
            match source.lookup(query) {
                Ok(found) => {
                    successes += 1;
                    let elapsed = started.elapsed().as_millis() as u64;
                    if elapsed > SLOW_SOURCE_MS {
                        sink.emit(RuntimeEvent::SourceSlow {
                            query_id: query_id.to_string(),
                            source_id: source.id().to_string(),
                            elapsed_ms: elapsed,
                        });
                    }
                    let mut deduped = dedupe_by_headword(found);
                    deduped.truncate(self.max_entries_per_source);
                    let is_empty = deduped.is_empty();
                    entries.extend(deduped);
                    entries.truncate(self.max_entries_total);
                    if self.strategy == QueryStrategy::FirstHit && !is_empty {
                        break;
                    }
                }
                Err(error) => {
                    failures += 1;
                    if failures == 1 {
                        first_failure_code = error.code();
                        first_failure_message = error.to_string();
                    }
                    tracing::warn!(
                        code = error.code(),
                        source_id = %source.id(),
                        source_name = %source.name(),
                        "数据源查询失败，已隔离"
                    );
                    sink.emit(RuntimeEvent::SourceFailure {
                        query_id: query_id.to_string(),
                        source_id: source.id().to_string(),
                        source_name: source.name().to_string(),
                        code: error.code().to_string(),
                        message: error.to_string(),
                    });
                }
            }
        }

        if successes == 0 && failures > 0 {
            return Err(IpcError::new(first_failure_code, first_failure_message));
        }
        Ok(entries)
    }

    fn select_sources(
        &self,
        source_ids: Option<&[String]>,
    ) -> Result<Vec<&'a std::sync::Arc<dyn DictionarySource>>, IpcError> {
        let mut sources = self.registry.enabled_sources();
        if let Some(ids) = source_ids {
            for id in ids {
                if !sources.iter().any(|source| source.id() == id) {
                    return Err(IpcError::new(
                        "SOURCE_NOT_FOUND",
                        format!("数据源不存在或未启用: {id}"),
                    ));
                }
            }
            sources.retain(|source| ids.iter().any(|id| id == source.id()));
        }
        Ok(sources)
    }
}

/// 同一 source 内按 headword 去重（NFKC 小写），保留首次出现顺序。
fn dedupe_by_headword(entries: Vec<Entry>) -> Vec<Entry> {
    let mut seen: Vec<String> = Vec::new();
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries {
        let key = entry.headword.trim().to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        result.push(entry);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Settings;
    use crate::core::entry::SourceConfig;
    use crate::core::error::SourceError;
    use crate::core::runtime::RecordingSink;
    use crate::sources::{SourceResource, SourceStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeSource {
        config: SourceConfig,
        entries: Vec<Entry>,
        fail: bool,
        calls: AtomicUsize,
    }

    impl FakeSource {
        fn new(id: &str, priority: i32, entries: Vec<Entry>) -> Self {
            Self {
                config: SourceConfig {
                    id: id.to_string(),
                    name: format!("name-{id}"),
                    kind: SourceKind::LocalFile { paths: vec![] },
                    enabled: true,
                    priority,
                },
                entries,
                fail: false,
                calls: AtomicUsize::new(0),
            }
        }

        fn failing(id: &str, priority: i32) -> Self {
            let mut source = Self::new(id, priority, Vec::new());
            source.fail = true;
            source
        }
    }

    use crate::core::entry::SourceKind;

    impl DictionarySource for FakeSource {
        fn id(&self) -> &str {
            &self.config.id
        }
        fn name(&self) -> &str {
            &self.config.name
        }
        fn priority(&self) -> i32 {
            self.config.priority
        }
        fn enabled(&self) -> bool {
            true
        }
        fn status(&self) -> SourceStatus {
            SourceStatus::Ready { entry_count: 1 }
        }
        fn lookup(&self, _query: &str) -> Result<Vec<Entry>, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(SourceError::Unavailable("down".to_string()))
            } else {
                let mut entries = self.entries.clone();
                for entry in &mut entries {
                    entry.source_name = self.config.name.clone();
                }
                Ok(entries)
            }
        }
        fn resolve_resource(
            &self,
            _dict_id: &str,
            _path: &str,
        ) -> Result<SourceResource, SourceError> {
            Err(SourceError::Unavailable("no resource".to_string()))
        }
    }

    fn entry(source: &str, headword: &str) -> Entry {
        let mut entry = Entry::new(headword, "<p>x</p>");
        entry.source_name = source.to_string();
        entry
    }

    fn settings(strategy: QueryStrategy) -> Settings {
        let mut settings = Settings::default();
        settings.general.query_strategy = strategy;
        settings.general.max_entries_per_source = 2;
        settings.general.max_entries_total = 20;
        settings
    }

    fn registry(sources: Vec<FakeSource>) -> SourceRegistry {
        let mut registry = SourceRegistry::empty();
        for source in sources {
            registry.register(source.config.clone(), std::sync::Arc::new(source));
        }
        registry
    }

    #[test]
    fn aggregate_collects_all_sources_in_priority_order() {
        let registry = registry(vec![
            FakeSource::new("b", 20, vec![entry("b", "hello")]),
            FakeSource::new("a", 10, vec![entry("a", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let entries = router.lookup("hello", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source_name, "name-a");
        assert_eq!(entries[1].source_name, "name-b");
        assert!(sink.events().is_empty());
    }

    #[test]
    fn first_hit_stops_after_first_non_empty_source() {
        let registry = registry(vec![
            FakeSource::new("a", 10, vec![entry("a", "hello")]),
            FakeSource::new("b", 20, vec![entry("b", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::FirstHit));
        let sink = RecordingSink::new();
        let entries = router.lookup("hello", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_name, "name-a");
    }

    #[test]
    fn local_first_keeps_all_local_results() {
        let registry = registry(vec![
            FakeSource::new("a", 10, vec![entry("a", "hello")]),
            FakeSource::new("b", 20, vec![entry("b", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::LocalFirst));
        let sink = RecordingSink::new();
        let entries = router.lookup("hello", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn failures_are_isolated_and_reported() {
        let registry = registry(vec![
            FakeSource::failing("bad", 5),
            FakeSource::new("good", 10, vec![entry("good", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let entries = router.lookup("hello", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 1);
        assert_eq!(sink.failures().len(), 1);
    }

    #[test]
    fn all_failures_return_error() {
        let registry = registry(vec![
            FakeSource::failing("a", 1),
            FakeSource::failing("b", 2),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let error = router.lookup("hello", None, &sink).expect_err("error");
        assert_eq!(error.code, "DICT_NOT_READY");
    }

    #[test]
    fn per_source_and_total_limits_apply() {
        let registry = registry(vec![FakeSource::new(
            "a",
            1,
            vec![entry("a", "one"), entry("a", "two"), entry("a", "three")],
        )]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let entries = router.lookup("x", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn dedupes_same_headword_within_source() {
        let registry = registry(vec![FakeSource::new(
            "a",
            1,
            vec![entry("a", "Hello"), entry("a", "hello")],
        )]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let entries = router.lookup("x", None, &sink).expect("lookup");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn explicit_unknown_source_id_is_rejected() {
        let registry = registry(vec![FakeSource::new("a", 1, vec![])]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let error = router
            .lookup("x", Some(&["missing".to_string()]), &sink)
            .expect_err("error");
        assert_eq!(error.code, "SOURCE_NOT_FOUND");
    }
}
