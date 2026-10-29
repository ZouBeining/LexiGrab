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
use crate::core::query_router::QueryRouter;
use crate::core::registry::{SourceRegistry, SourceStateEntry};
use crate::core::runtime::EventSink;
use crate::db::CardStore;
use crate::platform::{CaptureMethod, CaptureResult, SelectionContext};

/// 捕获样本上限，避免诊断数据无限增长。
const MAX_SAMPLES: usize = 200;
/// QueryMeta 环形缓存只保留最近 8 个 query_id。
pub const MAX_QUERY_META: usize = 8;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CachedSource {
    pub source_id: String,
    pub source_name: String,
}

/// `get_query_meta` 的返回结构，M2 还没有远程源与缓存命中。
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
    hotkey: RwLock<HotkeyStatus>,
    last_capture: Mutex<Option<CaptureResult>>,
    samples: Mutex<Vec<CaptureSample>>,
    query_meta: Mutex<VecDeque<QueryMeta>>,
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

        let registry = SourceRegistry::empty();
        let accelerator = settings.general.hotkey.clone();
        Ok(Self {
            paths,
            config,
            settings: RwLock::new(settings),
            registry: RwLock::new(registry),
            cards: Arc::new(cards),
            hotkey: RwLock::new(HotkeyStatus {
                accelerator,
                registered: false,
                message: None,
            }),
            last_capture: Mutex::new(None),
            samples: Mutex::new(Vec::new()),
            query_meta: Mutex::new(VecDeque::new()),
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

    /// 同步查询；调用方用 `spawn_blocking` 包住。查询结果会写入 QueryMeta 环形缓存。
    pub fn lookup(
        &self,
        query: &str,
        source_ids: Option<&[String]>,
        query_id: &str,
        sink: &dyn EventSink,
    ) -> IpcResult<Vec<Entry>> {
        let query = query.trim();
        if query.is_empty() {
            return Err(crate::core::error::IpcError::new(
                "QUERY_EMPTY",
                "请输入要查询的单词",
            ));
        }
        let settings = self.settings();
        let registry = read(&self.registry);
        let entries = QueryRouter::new(&registry, &settings).lookup(query, source_ids, sink)?;
        drop(registry);
        self.record_query_meta(QueryMeta {
            query_id: query_id.to_string(),
            cached_sources: Vec::new(),
            pending_remote: 0,
        });
        Ok(entries)
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
            hotkey: RwLock::new(HotkeyStatus {
                accelerator: "CmdOrCtrl+Shift+D".to_string(),
                registered: false,
                message: None,
            }),
            last_capture: Mutex::new(None),
            samples: Mutex::new(Vec::new()),
            query_meta: Mutex::new(VecDeque::new()),
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
}
