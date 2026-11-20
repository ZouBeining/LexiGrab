// SPDX-License-Identifier: AGPL-3.0-only
//! 应用运行时状态：配置、数据源注册表、卡片历史、捕获诊断与查询元数据。
//!
//! 所有字段都可从 `&AppState` 读取，内部用锁保护；命令层只做薄封装。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::config::{ConfigStore, Paths, Settings};
use crate::core::entry::Entry;
use crate::core::error::IpcResult;
use crate::core::query_router::{CachedSource, LookupOutcome, QueryRouter, RemoteJob};
use crate::core::registry::{SourceRegistry, SourceStateEntry};
use crate::core::runtime::EventSink;
use crate::db::cache::CacheStore;
use crate::db::CardStore;
use crate::platform::{CaptureMethod, CaptureResult, SelectionContext};

/// 捕获样本上限，避免诊断数据无限增长。
const MAX_SAMPLES: usize = 200;
/// QueryMeta 环形缓存只保留最近 8 个 query_id。
pub const MAX_QUERY_META: usize = 8;
/// 桥接语境查询环形缓存上限，与 QueryMeta 一致。
pub const MAX_SENSE_QUERIES: usize = 8;

/// `lookup_collect_all` 的结果：已收集条目、贡献来源名与是否因截止时间中断。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollectAllResult {
    pub entries: Vec<Entry>,
    pub source_names: Vec<String>,
    pub partial: bool,
}

/// 桥接 `/v1/sense/{query_id}` 的状态；M4 会写入 `Resolved`。
#[derive(Debug, Clone, PartialEq)]
pub enum SenseQueryState {
    Pending,
    Resolved(serde_json::Value),
}

#[derive(Debug, Clone, Serialize)]
pub struct HotkeyStatus {
    pub accelerator: String,
    pub registered: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureSample {
    pub elapsed_ms: f64,
    pub method: Option<CaptureMethod>,
    pub app_name: String,
    pub succeeded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureStats {
    pub samples: Vec<CaptureSample>,
    pub p50_ms: Option<f64>,
    pub p95_ms: Option<f64>,
    pub success_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeStatus {
    pub version: String,
    pub hotkey: HotkeyStatus,
    pub config_dir: String,
    pub log_dir: String,
    pub cache_path: String,
    pub popup_visible: bool,
    pub sources_ready: usize,
}

/// `get_query_meta` 的返回结构：本次查询命中的缓存来源与仍在执行的远程任务数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryMeta {
    pub query_id: String,
    pub cached_sources: Vec<CachedSource>,
    pub pending_remote: u32,
}

pub struct AppState {
    pub paths: Paths,
    pub config: ConfigStore,
    settings: RwLock<Settings>,
    registry: RwLock<SourceRegistry>,
    cards: Arc<CardStore>,
    cache: Arc<CacheStore>,
    hotkey: RwLock<HotkeyStatus>,
    last_capture: Mutex<Option<CaptureResult>>,
    samples: Mutex<Vec<CaptureSample>>,
    query_meta: Mutex<VecDeque<QueryMeta>>,
    sense_queries: Mutex<VecDeque<(String, SenseQueryState)>>,
    popup_interactive: AtomicBool,
    last_trigger: Mutex<Option<Instant>>,
    config_warning: Option<String>,
}

/// 取锁失败时复用内部数据，避免一次 panic 让整个功能不可用。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl AppState {
    /// 加载配置、打开卡片库；任何一步降级都不阻塞启动，只记录警告。
    pub fn new(paths: Paths) -> IpcResult<Self> {
        let config = ConfigStore::from_paths(&paths);
        let loaded = config.load();
        let settings = loaded.settings;
        let config_warning = loaded.warning;

        let cards = match CardStore::open(&paths.cache_db) {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!(error = %error, path = %paths.cache_db.display(), "打开卡片库失败，改用内存库");
                CardStore::in_memory().map_err(|fallback| {
                    crate::core::error::IpcError::internal(format!(
                        "无法打开卡片库: {error}; 内存库也不可用: {fallback}"
                    ))
                })?
            }
        };

        let cache = match CacheStore::open(&paths.cache_db) {
            Ok(store) => store,
            Err(error) => {
                tracing::warn!(error = %error, path = %paths.cache_db.display(), "打开查询缓存失败，改用内存缓存");
                CacheStore::in_memory().map_err(|fallback| {
                    crate::core::error::IpcError::internal(format!(
                        "无法打开查询缓存: {error}; 内存缓存也不可用: {fallback}"
                    ))
                })?
            }
        };

        let registry = SourceRegistry::empty();
        let accelerator = settings.general.hotkey.clone();
        Ok(Self {
            paths,
            config,
            settings: RwLock::new(settings),
            registry: RwLock::new(registry),
            cards: Arc::new(cards),
            cache: Arc::new(cache),
            hotkey: RwLock::new(HotkeyStatus {
                accelerator,
                registered: false,
                message: None,
            }),
            last_capture: Mutex::new(None),
            samples: Mutex::new(Vec::new()),
            query_meta: Mutex::new(VecDeque::new()),
            sense_queries: Mutex::new(VecDeque::new()),
            popup_interactive: AtomicBool::new(false),
            last_trigger: Mutex::new(None),
            config_warning,
        })
    }

    pub fn config_warning(&self) -> Option<String> {
        self.config_warning.clone()
    }

    pub fn settings(&self) -> Settings {
        read(&self.settings).clone()
    }

    /// 校验、写盘并替换内存配置；失败时不写盘也不改内存。
    pub fn replace_settings(&self, settings: Settings) -> IpcResult<Settings> {
        settings.validate()?;
        self.config.save(&settings)?;
        *write(&self.settings) = settings.clone();
        Ok(settings)
    }

    /// 用当前配置重建注册表。调用方负责放到后台线程，避免阻塞查询。
    pub fn rebuild_registry(&self, settings: &Settings) {
        let registry = SourceRegistry::from_settings(settings, &self.paths.config_dir);
        *write(&self.registry) = registry;
    }

    pub fn source_states(&self) -> Vec<SourceStateEntry> {
        read(&self.registry).states()
    }

    /// 读取本地资源，供 `mdx-res` 协议使用。
    pub fn resolve_resource(
        &self,
        dict_id: &str,
        path: &str,
    ) -> Result<crate::sources::SourceResource, crate::core::resources::ResourceError> {
        let parsed = crate::core::resources::ParsedResource {
            dict_id: dict_id.to_string(),
            path: path.to_string(),
        };
        let registry = read(&self.registry);
        crate::core::resources::resolve_resource(&registry, &parsed)
    }

    pub fn sources_ready(&self) -> usize {
        read(&self.registry)
            .states()
            .iter()
            .filter(|entry| entry.status.is_ready())
            .count()
    }

    pub fn cards(&self) -> Arc<CardStore> {
        Arc::clone(&self.cards)
    }

    /// 查询缓存；设置页统计与语境辅助共用。
    pub fn cache(&self) -> Arc<CacheStore> {
        Arc::clone(&self.cache)
    }

    /// 同步查询；调用方用 `spawn_blocking` 包住。查询结果会写入 QueryMeta 环形缓存。
    pub fn lookup(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        query_id: &str,
        sink: &dyn EventSink,
    ) -> IpcResult<Vec<Entry>> {
        Ok(self
            .lookup_planned(query, source_ids, query_id, sink)?
            .entries)
    }

    /// 同步查询并返回待后台执行的远程任务。
    pub fn lookup_planned(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        query_id: &str,
        sink: &dyn EventSink,
    ) -> IpcResult<LookupOutcome> {
        let query = query.trim();
        if query.is_empty() {
            return Err(crate::core::error::IpcError::new(
                "QUERY_EMPTY",
                "请输入要查询的单词",
            ));
        }
        let settings = self.settings();
        let outcome = {
            let registry = read(&self.registry);
            QueryRouter::new(&registry, &settings)
                .with_cache(&self.cache)
                .lookup_planned(query, source_ids, query_id, sink)?
        };
        self.record_query_meta(QueryMeta {
            query_id: query_id.to_string(),
            cached_sources: outcome.cached_sources.clone(),
            pending_remote: outcome.remote_jobs.len() as u32,
        });
        Ok(outcome)
    }

    /// 远程任务完成（成功或失败）后递减 pending，并记录新命中的缓存来源。
    pub fn complete_remote(&self, query_id: &str, cached_source: Option<CachedSource>) {
        let mut ring = lock(&self.query_meta);
        let Some(meta) = ring.iter_mut().find(|meta| meta.query_id == query_id) else {
            return;
        };
        meta.pending_remote = meta.pending_remote.saturating_sub(1);
        if let Some(cached) = cached_source {
            if !meta
                .cached_sources
                .iter()
                .any(|existing| existing.source_id == cached.source_id)
            {
                meta.cached_sources.push(cached);
            }
        }
    }

    /// 执行一个远程任务并广播事件；命令层放到后台线程调用。
    pub fn run_remote_job(
        &self,
        job: &RemoteJob,
        query: &str,
        query_id: &str,
        sink: &dyn EventSink,
    ) {
        let settings = self.settings();
        let mut outcome = LookupOutcome::default();
        {
            let registry = read(&self.registry);
            QueryRouter::new(&registry, &settings)
                .with_cache(&self.cache)
                .run_remote_job(job, query, query_id, &mut outcome, sink);
        }
        self.complete_remote(query_id, None);
    }

    fn record_query_meta(&self, meta: QueryMeta) {
        let mut ring = lock(&self.query_meta);
        ring.retain(|existing| existing.query_id != meta.query_id);
        ring.push_back(meta);
        while ring.len() > MAX_QUERY_META {
            ring.pop_front();
        }
    }

    pub fn query_meta(&self, query_id: &str) -> Option<QueryMeta> {
        lock(&self.query_meta)
            .iter()
            .find(|meta| meta.query_id == query_id)
            .cloned()
    }

    /// 并发查询全部启用本地源，按 `deadline` 截止；未赶上的源计入 `partial`。
    ///
    /// 桥接的 `lookup_collect_all` 用它：每个源一个线程（源查询本身已有 500ms
    /// 超时），主线程用 `recv_timeout` 等剩余时间，到点即返回已收集结果。
    /// 未完成的线程自行结束，结果丢弃。
    pub fn lookup_collect_all(&self, query: &str, deadline: Duration) -> CollectAllResult {
        let settings = self.settings();
        let sources = read(&self.registry).enabled_source_handles();
        if sources.is_empty() {
            return CollectAllResult::default();
        }

        let (sender, receiver) = std::sync::mpsc::channel();
        let expected = sources.len();
        let per_source = settings.general.max_entries_per_source as usize;
        let total = settings.general.max_entries_total as usize;
        let positive_ttl = i64::from(settings.cache.ttl_hours) * 3600;
        let negative_ttl = i64::from(settings.cache.negative_ttl_minutes) * 60;
        for source in sources {
            let sender = sender.clone();
            let query = query.to_string();
            let cache = Arc::clone(&self.cache);
            let remote = source.is_remote();
            std::thread::spawn(move || {
                if remote {
                    if let Ok(Some(hit)) = cache.get(source.id(), &query) {
                        // 负缓存命中也算一个结果（空），让桥接走 partial 逻辑。
                        let _ = sender.send((source.name().to_string(), Ok(hit.entries)));
                        return;
                    }
                }
                let result = source.lookup(&query);
                if remote {
                    if let Ok(entries) = &result {
                        if entries.is_empty() {
                            let _ = cache.put_empty(source.id(), &query, negative_ttl);
                        } else {
                            let _ = cache.put_entries(source.id(), &query, entries, positive_ttl);
                        }
                    }
                }
                let _ = sender.send((source.name().to_string(), result));
            });
        }
        drop(sender);

        let started = Instant::now();
        let mut collected = CollectAllResult::default();
        let mut answered = 0usize;
        while answered < expected {
            if collected.entries.len() >= total {
                // 达到总条数上限：剩余源不必再等，结果按计划截断。
                collected.partial = true;
                break;
            }
            let remaining = deadline.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                collected.partial = true;
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok((name, Ok(entries))) => {
                    answered += 1;
                    if !entries.is_empty()
                        && !collected.source_names.iter().any(|known| known == &name)
                    {
                        collected.source_names.push(name);
                    }
                    for entry in entries.into_iter().take(per_source) {
                        if collected.entries.len() >= total {
                            break;
                        }
                        collected.entries.push(entry);
                    }
                }
                Ok((_, Err(error))) => {
                    answered += 1;
                    tracing::warn!(code = error.code(), "桥接查询单源失败，已隔离");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    collected.partial = true;
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    collected.partial = true;
                    break;
                }
            }
        }
        collected
    }

    /// 登记一个进行中的语境查询；M4 完成后用 [`Self::resolve_sense`] 写入结果。
    pub fn mark_sense_pending(&self, query_id: &str) {
        self.set_sense_state(query_id, SenseQueryState::Pending);
    }

    /// 写入已完成的语境结果（M4 使用）。
    pub fn resolve_sense(&self, query_id: &str, payload: serde_json::Value) {
        self.set_sense_state(query_id, SenseQueryState::Resolved(payload));
    }

    /// 后台解析语境推荐：选择器 →（低置信度时）LLM，结果只作为建议。
    pub fn run_sense_resolution(
        &self,
        query_id: &str,
        headword: &str,
        sentence: Option<&str>,
        entries: &[Entry],
    ) -> crate::core::sense::SenseResolution {
        let settings = self.settings();
        crate::core::sense::resolve(crate::core::sense::SenseInput {
            query_id,
            headword,
            sentence,
            entries,
            settings: &settings,
            cache: &self.cache,
        })
    }

    /// 把语境结果写成事件/桥接共用的 JSON；失败只记录日志。
    pub fn store_sense_resolution(
        &self,
        query_id: &str,
        resolution: &crate::core::sense::SenseResolution,
    ) {
        match serde_json::to_value(resolution) {
            Ok(value) => self.resolve_sense(query_id, value),
            Err(error) => tracing::warn!(error = %error, "序列化语境结果失败"),
        }
    }

    /// 读取已完成或进行中的语境结果；进行中返回 `None`。
    pub fn sense_resolution(&self, query_id: &str) -> Option<crate::core::sense::SenseResolution> {
        match self.sense_query_state(query_id) {
            Some(SenseQueryState::Resolved(value)) => serde_json::from_value(value).ok(),
            _ => None,
        }
    }

    pub fn sense_query_state(&self, query_id: &str) -> Option<SenseQueryState> {
        lock(&self.sense_queries)
            .iter()
            .find(|(id, _)| id == query_id)
            .map(|(_, state)| state.clone())
    }

    fn set_sense_state(&self, query_id: &str, state: SenseQueryState) {
        let mut ring = lock(&self.sense_queries);
        ring.retain(|(existing, _)| existing != query_id);
        ring.push_back((query_id.to_string(), state));
        while ring.len() > MAX_SENSE_QUERIES {
            ring.pop_front();
        }
    }

    pub fn hotkey_status(&self) -> HotkeyStatus {
        read(&self.hotkey).clone()
    }

    pub fn set_hotkey_status(&self, accelerator: &str, registered: bool, message: Option<String>) {
        let mut status = write(&self.hotkey);
        status.accelerator = accelerator.to_string();
        status.registered = registered;
        status.message = message;
    }

    pub fn record_capture(&self, result: &CaptureResult) {
        {
            let mut last = lock(&self.last_capture);
            *last = Some(result.clone());
        }
        let sample = match &result.selection {
            Some(selection) => CaptureSample {
                elapsed_ms: selection.elapsed_ms,
                method: Some(selection.method),
                app_name: selection.app_name.clone(),
                succeeded: true,
            },
            None => CaptureSample {
                elapsed_ms: 0.0,
                method: None,
                app_name: String::new(),
                succeeded: false,
            },
        };
        let mut samples = lock(&self.samples);
        samples.push(sample);
        if samples.len() > MAX_SAMPLES {
            samples.remove(0);
        }
    }

    pub fn last_selection(&self) -> Option<SelectionContext> {
        lock(&self.last_capture)
            .as_ref()
            .and_then(|result| result.selection.clone())
    }

    /// 句子捕获异步返回后补写最近一次选区；文本不匹配说明已有新选区，忽略。
    pub fn update_sentence(
        &self,
        text: &str,
        sentence: Option<String>,
    ) -> Option<SelectionContext> {
        let mut guard = lock(&self.last_capture);
        let result = guard.as_mut()?;
        let selection = result.selection.as_mut()?;
        if selection.text != text {
            return None;
        }
        if selection.sentence.is_none() {
            selection.sentence = sentence;
        }
        Some(selection.clone())
    }

    pub fn last_capture(&self) -> Option<CaptureResult> {
        lock(&self.last_capture).clone()
    }

    pub fn capture_stats(&self) -> CaptureStats {
        let samples = lock(&self.samples).clone();
        let mut durations: Vec<f64> = samples
            .iter()
            .filter(|sample| sample.succeeded)
            .map(|sample| sample.elapsed_ms)
            .collect();
        durations.sort_by(|left, right| left.total_cmp(right));
        let succeeded = samples.iter().filter(|sample| sample.succeeded).count();
        CaptureStats {
            p50_ms: percentile(&durations, 0.50),
            p95_ms: percentile(&durations, 0.95),
            success_rate: if samples.is_empty() {
                None
            } else {
                Some(succeeded as f64 / samples.len() as f64)
            },
            samples,
        }
    }

    pub fn set_popup_interactive(&self, interactive: bool) {
        self.popup_interactive.store(interactive, Ordering::SeqCst);
    }

    pub fn popup_interactive(&self) -> bool {
        self.popup_interactive.load(Ordering::SeqCst)
    }

    /// 250ms 去抖：返回 true 表示本次触发应当执行。
    pub fn allow_trigger(&self, window: Duration) -> bool {
        let now = Instant::now();
        let mut last = lock(&self.last_trigger);
        match *last {
            Some(previous) if now.duration_since(previous) < window => false,
            _ => {
                *last = Some(now);
                true
            }
        }
    }
}

/// 最近秩百分位（nearest-rank），样本为空时返回 None。
fn percentile(sorted: &[f64], ratio: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (sorted.len() as f64 * ratio).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::entry::Entry;

    fn state() -> AppState {
        AppState {
            paths: Paths {
                config_dir: std::path::PathBuf::from("target/test-tmp/state/config"),
                log_dir: std::path::PathBuf::from("target/test-tmp/state/logs"),
                cache_db: std::path::PathBuf::from("target/test-tmp/state/cache/cache.db"),
            },
            config: ConfigStore::with_config_path("target/test-tmp/state/config/config.toml"),
            settings: RwLock::new(Settings::default()),
            registry: RwLock::new(SourceRegistry::empty()),
            cards: Arc::new(CardStore::in_memory().expect("in-memory card store")),
            cache: Arc::new(CacheStore::in_memory().expect("in-memory cache store")),
            hotkey: RwLock::new(HotkeyStatus {
                accelerator: "CmdOrCtrl+Shift+D".to_string(),
                registered: false,
                message: None,
            }),
            last_capture: Mutex::new(None),
            samples: Mutex::new(Vec::new()),
            query_meta: Mutex::new(VecDeque::new()),
            sense_queries: Mutex::new(VecDeque::new()),
            popup_interactive: AtomicBool::new(false),
            last_trigger: Mutex::new(None),
            config_warning: None,
        }
    }

    #[test]
    fn percentile_handles_empty_and_single_sample() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[12.0], 0.95), Some(12.0));
    }

    #[test]
    fn percentile_uses_nearest_rank() {
        let samples = [10.0, 20.0, 30.0, 40.0];
        assert_eq!(percentile(&samples, 0.5), Some(20.0));
        assert_eq!(percentile(&samples, 0.95), Some(40.0));
    }

    #[test]
    fn debounce_ignores_repeat_within_window() {
        let state = state();
        let window = Duration::from_millis(250);
        assert!(state.allow_trigger(window));
        assert!(!state.allow_trigger(window));
    }

    #[test]
    fn debounce_allows_after_window() {
        let state = state();
        assert!(state.allow_trigger(Duration::from_millis(0)));
    }

    #[test]
    fn query_meta_ring_keeps_latest_eight() {
        let state = state();
        for index in 0..10 {
            state.record_query_meta(QueryMeta {
                query_id: format!("q{index}"),
                cached_sources: Vec::new(),
                pending_remote: 0,
            });
        }
        assert!(state.query_meta("q0").is_none());
        assert!(state.query_meta("q1").is_none());
        assert!(state.query_meta("q9").is_some());
    }

    #[test]
    fn empty_query_is_rejected() {
        let state = state();
        let sink = crate::core::runtime::NullSink;
        let error = state
            .lookup("   ", None, "q1", &sink)
            .expect_err("empty query");
        assert_eq!(error.code, "QUERY_EMPTY");
    }

    #[test]
    fn lookup_without_sources_returns_empty_entries() {
        let state = state();
        let sink = crate::core::runtime::NullSink;
        let entries: Vec<Entry> = state.lookup("hello", None, "q1", &sink).expect("lookup");
        assert!(entries.is_empty());
        assert_eq!(
            state.query_meta("q1").map(|meta| meta.query_id),
            Some("q1".to_string())
        );
    }

    #[test]
    fn collect_all_without_sources_is_complete_and_empty() {
        let state = state();
        let collected = state.lookup_collect_all("hello", Duration::from_millis(50));
        assert!(collected.entries.is_empty());
        assert!(collected.source_names.is_empty());
        assert!(!collected.partial);
    }

    #[test]
    fn sense_query_states_are_pending_then_resolved() {
        let state = state();
        assert!(state.sense_query_state("q1").is_none());
        state.mark_sense_pending("q1");
        assert_eq!(
            state.sense_query_state("q1"),
            Some(SenseQueryState::Pending)
        );
        state.resolve_sense("q1", serde_json::json!({ "status": "selected" }));
        assert_eq!(
            state.sense_query_state("q1"),
            Some(SenseQueryState::Resolved(
                serde_json::json!({ "status": "selected" })
            ))
        );
    }

    #[test]
    fn sense_query_ring_keeps_latest_eight() {
        let state = state();
        for index in 0..10 {
            state.mark_sense_pending(&format!("q{index}"));
        }
        assert!(state.sense_query_state("q1").is_none());
        assert!(state.sense_query_state("q9").is_some());
    }
}
