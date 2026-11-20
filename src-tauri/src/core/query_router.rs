// SPDX-License-Identifier: AGPL-3.0-only
//! 查询路由：三种策略、缓存、远程后台任务、失败隔离与条数上限。
//!
//! 本地源同步查询；远程源按策略处理——`first_hit` 为保持“首个命中即停止”
//! 的语义同步等待，`local_first` / `aggregate` 返回已缓存的远程结果并给出
//! 待后台执行的远程任务。调用方（命令层）负责把任务放到后台线程并通过
//! [`RuntimeEvent::RemoteResult`] / [`RuntimeEvent::RemoteError`] 通知前端。

use std::time::Instant;

use serde::Serialize;

use crate::core::config::{QueryStrategy, Settings};
use crate::core::entry::Entry;
use crate::core::error::IpcError;
use crate::core::registry::SourceRegistry;
use crate::core::runtime::{EventSink, QueryId, RuntimeEvent};
use crate::db::cache::CacheStore;
use crate::sources::DictionarySource;

/// 慢查询阈值：超过则发 SourceSlow 事件。
const SLOW_SOURCE_MS: u64 = 500;

/// 缓存命中的来源，供 `QueryMeta` 展示。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CachedSource {
    pub source_id: String,
    pub source_name: String,
}

/// 需要后台执行的远程查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteJob {
    pub source_id: String,
    pub source_name: String,
}

/// 一次查询的同步结果与待办远程任务。
#[derive(Debug, Clone, Default)]
pub struct LookupOutcome {
    pub entries: Vec<Entry>,
    pub cached_sources: Vec<CachedSource>,
    pub remote_jobs: Vec<RemoteJob>,
}

pub struct QueryRouter<'a> {
    registry: &'a SourceRegistry,
    cache: Option<&'a CacheStore>,
    strategy: QueryStrategy,
    max_entries_per_source: usize,
    max_entries_total: usize,
    positive_ttl_secs: i64,
    negative_ttl_secs: i64,
}

impl<'a> QueryRouter<'a> {
    pub fn new(registry: &'a SourceRegistry, settings: &Settings) -> Self {
        Self {
            registry,
            cache: None,
            strategy: settings.general.query_strategy,
            max_entries_per_source: settings.general.max_entries_per_source as usize,
            max_entries_total: settings.general.max_entries_total as usize,
            positive_ttl_secs: i64::from(settings.cache.ttl_hours) * 3600,
            negative_ttl_secs: i64::from(settings.cache.negative_ttl_minutes) * 60,
        }
    }

    /// 接入 SQLite 缓存；未接入时远程源不读写缓存。
    pub fn with_cache(mut self, cache: &'a CacheStore) -> Self {
        self.cache = Some(cache);
        self
    }

    /// 兼容 M1/M2 的同步查询：远程任务（`first_hit` 之外）也在此同步执行。
    pub fn lookup(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        sink: &dyn EventSink,
    ) -> Result<Vec<Entry>, IpcError> {
        let query_id = QueryId::new().to_string();
        let mut outcome = self.lookup_planned(query, source_ids, &query_id, sink)?;
        let jobs = std::mem::take(&mut outcome.remote_jobs);
        for job in jobs {
            if outcome.entries.len() >= self.max_entries_total {
                break;
            }
            self.run_remote_job(&job, query, &query_id, &mut outcome, sink);
        }
        Ok(outcome.entries)
    }

    /// 同步查询 + 返回待后台执行的远程任务。
    pub fn lookup_planned(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        query_id: &str,
        sink: &dyn EventSink,
    ) -> Result<LookupOutcome, IpcError> {
        let sources = self.select_sources(source_ids)?;
        let mut outcome = LookupOutcome::default();
        let mut successes = 0usize;
        let mut failures = 0usize;
        let mut first_failure_code = "INTERNAL";
        let mut first_failure_message = "所有词典源查询失败".to_string();

        for source in sources {
            if outcome.entries.len() >= self.max_entries_total {
                break;
            }

            if source.is_remote() {
                match self.apply_cached_remote(source, query, query_id, &mut outcome, sink) {
                    Some(true) => {
                        // 正缓存命中：本次查询有结果。
                        successes += 1;
                        if self.strategy == QueryStrategy::FirstHit {
                            break;
                        }
                        continue;
                    }
                    Some(false) => {
                        // 负缓存命中：跳过该源，继续下一个。
                        continue;
                    }
                    None => {}
                }
            }

            if source.is_remote() && self.strategy != QueryStrategy::FirstHit {
                // 未命中缓存：交给调用方后台查询，避免拖慢首屏。
                outcome.remote_jobs.push(RemoteJob {
                    source_id: source.id().to_string(),
                    source_name: source.name().to_string(),
                });
                continue;
            }

            let started = Instant::now();
            match source.lookup(query) {
                Ok(found) => {
                    successes += 1;
                    let elapsed = started.elapsed().as_millis() as u64;
                    self.emit_slow(source, query_id, elapsed, sink);
                    let found = self.truncate_source(found);
                    let is_empty = found.is_empty();
                    if source.is_remote() {
                        self.write_cache(source.id(), query, &found);
                        sink.emit(RuntimeEvent::RemoteResult {
                            query_id: query_id.to_string(),
                            source_id: source.id().to_string(),
                            source_name: source.name().to_string(),
                            entries: found.clone(),
                            cached: false,
                            elapsed_ms: elapsed,
                        });
                    }
                    outcome.entries.extend(found);
                    outcome.entries.truncate(self.max_entries_total);
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
                    self.emit_source_failure(source, query_id, &error, sink);
                    if source.is_remote() {
                        sink.emit(RuntimeEvent::RemoteError {
                            query_id: query_id.to_string(),
                            source_id: source.id().to_string(),
                            code: error.code().to_string(),
                            message: error.to_string(),
                        });
                    }
                }
            }
        }

        // 还有远程任务待执行时先不报“全部失败”，让 UI 显示远程进度。
        if successes == 0 && failures > 0 && outcome.remote_jobs.is_empty() {
            return Err(IpcError::new(first_failure_code, first_failure_message));
        }
        Ok(outcome)
    }

    /// 执行一个远程任务（可在后台线程调用），把结果写入 `outcome` 并广播事件。
    pub fn run_remote_job(
        &self,
        job: &RemoteJob,
        query: &str,
        query_id: &str,
        outcome: &mut LookupOutcome,
        sink: &dyn EventSink,
    ) {
        let Some(source) = self.registry.source(&job.source_id) else {
            sink.emit(RuntimeEvent::RemoteError {
                query_id: query_id.to_string(),
                source_id: job.source_id.clone(),
                code: "SOURCE_NOT_FOUND".to_string(),
                message: "远程数据源已被移除".to_string(),
            });
            return;
        };
        let started = Instant::now();
        match source.lookup(query) {
            Ok(found) => {
                let elapsed = started.elapsed().as_millis() as u64;
                let found = self.truncate_source(found);
                self.write_cache(&job.source_id, query, &found);
                sink.emit(RuntimeEvent::RemoteResult {
                    query_id: query_id.to_string(),
                    source_id: job.source_id.clone(),
                    source_name: job.source_name.clone(),
                    entries: found.clone(),
                    cached: false,
                    elapsed_ms: elapsed,
                });
                if outcome.entries.len() < self.max_entries_total {
                    outcome.entries.extend(found);
                    outcome.entries.truncate(self.max_entries_total);
                }
            }
            Err(error) => {
                tracing::warn!(
                    code = error.code(),
                    source_id = %job.source_id,
                    "远程数据源查询失败，已隔离"
                );
                sink.emit(RuntimeEvent::RemoteError {
                    query_id: query_id.to_string(),
                    source_id: job.source_id.clone(),
                    code: error.code().to_string(),
                    message: error.to_string(),
                });
            }
        }
    }

    /// 读取远程缓存并写入结果；返回 true 表示命中（含负缓存）。
    fn apply_cached_remote(
        &self,
        source: &std::sync::Arc<dyn DictionarySource>,
        query: &str,
        query_id: &str,
        outcome: &mut LookupOutcome,
        sink: &dyn EventSink,
    ) -> Option<bool> {
        let cache = self.cache?;
        match cache.get(source.id(), query) {
            Ok(Some(hit)) => {
                if hit.entries.is_empty() {
                    return Some(false);
                }
                let entries = self.truncate_source(hit.entries);
                if entries.is_empty() {
                    return Some(false);
                }
                outcome.cached_sources.push(CachedSource {
                    source_id: source.id().to_string(),
                    source_name: source.name().to_string(),
                });
                sink.emit(RuntimeEvent::RemoteResult {
                    query_id: query_id.to_string(),
                    source_id: source.id().to_string(),
                    source_name: source.name().to_string(),
                    entries: entries.clone(),
                    cached: true,
                    elapsed_ms: 0,
                });
                outcome.entries.extend(entries);
                outcome.entries.truncate(self.max_entries_total);
                Some(true)
            }
            Ok(None) => None,
            Err(error) => {
                // 缓存故障不能影响查询：按未命中继续走网络。
                tracing::warn!(error = %error, source_id = %source.id(), "读取查询缓存失败");
                None
            }
        }
    }

    /// 写远程缓存；失败只记录日志。
    fn write_cache(&self, source_id: &str, query: &str, entries: &[Entry]) {
        let Some(cache) = self.cache else {
            return;
        };
        let result = if entries.is_empty() {
            cache.put_empty(source_id, query, self.negative_ttl_secs)
        } else {
            cache.put_entries(source_id, query, entries, self.positive_ttl_secs)
        };
        if let Err(error) = result {
            tracing::warn!(error = %error, source_id, "写入查询缓存失败");
        }
    }

    /// 同一 source 内按 headword 去重并截断到每源上限。
    fn truncate_source(&self, entries: Vec<Entry>) -> Vec<Entry> {
        let mut deduped = dedupe_by_headword(entries);
        deduped.truncate(self.max_entries_per_source);
        deduped
    }

    fn emit_slow(
        &self,
        source: &std::sync::Arc<dyn DictionarySource>,
        query_id: &str,
        elapsed_ms: u64,
        sink: &dyn EventSink,
    ) {
        if elapsed_ms > SLOW_SOURCE_MS {
            sink.emit(RuntimeEvent::SourceSlow {
                query_id: query_id.to_string(),
                source_id: source.id().to_string(),
                elapsed_ms,
            });
        }
    }

    fn emit_source_failure(
        &self,
        source: &std::sync::Arc<dyn DictionarySource>,
        query_id: &str,
        error: &crate::core::error::SourceError,
        sink: &dyn EventSink,
    ) {
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
    use crate::core::entry::{SourceConfig, SourceKind};
    use crate::core::error::SourceError;
    use crate::core::runtime::{NullSink, RecordingSink};
    use crate::sources::{SourceResource, SourceStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeSource {
        config: SourceConfig,
        entries: Vec<Entry>,
        fail: bool,
        remote: bool,
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
                remote: false,
                calls: AtomicUsize::new(0),
            }
        }

        fn remote(id: &str, priority: i32, entries: Vec<Entry>) -> Self {
            let mut source = Self::new(id, priority, entries);
            source.remote = true;
            source
        }

        fn failing(id: &str, priority: i32) -> Self {
            let mut source = Self::new(id, priority, Vec::new());
            source.fail = true;
            source
        }
    }

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
        fn is_remote(&self) -> bool {
            self.remote
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

    #[test]
    fn aggregate_remote_sources_are_deferred_to_background_jobs() {
        let registry = registry(vec![
            FakeSource::new("local", 10, vec![entry("local", "hello")]),
            FakeSource::remote("remote", 20, vec![entry("remote", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let outcome = router
            .lookup_planned("hello", None, "q1", &sink)
            .expect("lookup");
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.remote_jobs.len(), 1);
        assert_eq!(outcome.remote_jobs[0].source_id, "remote");
    }

    #[test]
    fn first_hit_remote_is_queried_synchronously() {
        let registry = registry(vec![
            FakeSource::remote("remote", 10, vec![entry("remote", "hello")]),
            FakeSource::new("local", 20, vec![entry("local", "hello")]),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::FirstHit));
        let sink = RecordingSink::new();
        let outcome = router
            .lookup_planned("hello", None, "q1", &sink)
            .expect("lookup");
        assert_eq!(outcome.entries.len(), 1);
        assert!(outcome.remote_jobs.is_empty());
        assert_eq!(outcome.entries[0].source_name, "name-remote");
        assert!(matches!(
            sink.events().as_slice(),
            [RuntimeEvent::RemoteResult { cached: false, .. }]
        ));
    }

    #[test]
    fn cached_remote_hit_skips_network_and_is_marked() {
        let registry = registry(vec![FakeSource::remote(
            "remote",
            10,
            vec![entry("remote", "hello")],
        )]);
        let cache = CacheStore::in_memory().expect("cache");
        cache
            .put_entries(
                "remote",
                "hello",
                &{
                    let mut cached = entry("remote", "hello");
                    cached.source_name = "name-remote".to_string();
                    vec![cached]
                },
                60,
            )
            .expect("seed cache");
        let router =
            QueryRouter::new(&registry, &settings(QueryStrategy::LocalFirst)).with_cache(&cache);
        let sink = RecordingSink::new();
        let outcome = router
            .lookup_planned("hello", None, "q1", &sink)
            .expect("lookup");
        assert_eq!(outcome.entries.len(), 1);
        assert!(outcome.remote_jobs.is_empty());
        assert_eq!(outcome.cached_sources.len(), 1);
        assert_eq!(outcome.cached_sources[0].source_id, "remote");
        assert!(matches!(
            sink.events().as_slice(),
            [RuntimeEvent::RemoteResult { cached: true, .. }]
        ));
    }

    #[test]
    fn negative_cache_skips_source_and_continues() {
        let registry = registry(vec![
            FakeSource::remote("remote", 10, vec![entry("remote", "hello")]),
            FakeSource::new("local", 20, vec![entry("local", "hello")]),
        ]);
        let cache = CacheStore::in_memory().expect("cache");
        cache.put_empty("remote", "hello", 60).expect("seed");
        let router =
            QueryRouter::new(&registry, &settings(QueryStrategy::FirstHit)).with_cache(&cache);
        let sink = NullSink;
        let outcome = router
            .lookup_planned("hello", None, "q1", &sink)
            .expect("lookup");
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].source_name, "name-local");
        assert!(outcome.remote_jobs.is_empty());
    }

    #[test]
    fn remote_failure_isolated_from_local_results() {
        let registry = registry(vec![
            FakeSource::new("local", 10, vec![entry("local", "hello")]),
            FakeSource::failing("remote", 20),
        ]);
        let router = QueryRouter::new(&registry, &settings(QueryStrategy::Aggregate));
        let sink = RecordingSink::new();
        let outcome = router
            .lookup_planned("hello", None, "q1", &sink)
            .expect("lookup");
        assert_eq!(outcome.entries.len(), 1);
        // 失败源不是远程时只发 SourceFailure。
        assert_eq!(sink.failures().len(), 1);
    }
}
