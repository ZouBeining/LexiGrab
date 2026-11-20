//! 数据源注册表：按 enabled、priority、id 稳定排序；reload 复用未变化的 worker。

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::core::config::Settings;
use crate::core::entry::{SourceConfig, SourceKind};
use crate::sources::{build_source, DictionarySource, SourceStatus};

/// 设置页/CLI 展示的数据源状态。
#[derive(Debug, Clone, Serialize)]
pub struct SourceStateEntry {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
    pub status: SourceStatus,
}

struct RegistryEntry {
    config: SourceConfig,
    source: Arc<dyn DictionarySource>,
}

/// 已打开的数据源集合（本地文件与远程 HTTP）。
#[derive(Default)]
pub struct SourceRegistry {
    entries: Vec<RegistryEntry>,
}

impl SourceRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// 打开配置中启用的数据源；配置有问题的远程源记录为 Failed，不阻塞启动。
    pub fn from_settings(settings: &Settings, base_dir: &Path) -> Self {
        let mut registry = Self::empty();
        registry.reload(settings, base_dir);
        registry
    }

    /// 重载配置：内容未变化的源复用原 worker，其余重建；旧 worker 随 Arc 释放回收。
    pub fn reload(&mut self, settings: &Settings, base_dir: &Path) {
        let mut previous: Vec<RegistryEntry> = std::mem::take(&mut self.entries);
        let mut entries = Vec::new();
        for config in settings.sources.iter() {
            if !config.enabled {
                continue;
            }
            if let SourceKind::RemoteHttp { .. } = &config.kind {
                tracing::debug!(source_id = %config.id, "打开远程源");
            }
            let reused = previous
                .iter()
                .position(|entry| entry.config == *config)
                .map(|index| previous.swap_remove(index));
            let source = match reused {
                Some(entry) => entry.source,
                None => build_source(config.clone(), base_dir),
            };
            entries.push(RegistryEntry {
                config: config.clone(),
                source,
            });
        }
        // 剩余未复用的旧 worker 在此 drop。
        drop(previous);
        entries.sort_by(|left, right| {
            left.config
                .priority
                .cmp(&right.config.priority)
                .then_with(|| left.config.id.cmp(&right.config.id))
        });
        self.entries = entries;
    }

    /// 注册一个已构造的数据源（测试与 M2 远程源使用）。
    pub fn register(&mut self, config: SourceConfig, source: Arc<dyn DictionarySource>) {
        self.entries.push(RegistryEntry { config, source });
        self.entries.sort_by(|left, right| {
            left.config
                .priority
                .cmp(&right.config.priority)
                .then_with(|| left.config.id.cmp(&right.config.id))
        });
    }

    pub fn source(&self, id: &str) -> Option<&Arc<dyn DictionarySource>> {
        self.entries
            .iter()
            .find(|entry| entry.config.id == id)
            .map(|entry| &entry.source)
    }

    /// 启用且已打开的数据源，按 (priority, id) 稳定排序。
    pub fn enabled_sources(&self) -> Vec<&Arc<dyn DictionarySource>> {
        self.entries
            .iter()
            .filter(|entry| entry.config.enabled)
            .map(|entry| &entry.source)
            .collect()
    }

    /// 拥有所有权的启用源快照，供并发查询（桥接 `lookup_collect_all`）使用。
    pub fn enabled_source_handles(&self) -> Vec<Arc<dyn DictionarySource>> {
        self.entries
            .iter()
            .filter(|entry| entry.config.enabled)
            .map(|entry| Arc::clone(&entry.source))
            .collect()
    }

    pub fn states(&self) -> Vec<SourceStateEntry> {
        self.entries
            .iter()
            .map(|entry| SourceStateEntry {
                id: entry.config.id.clone(),
                name: entry.config.name.clone(),
                enabled: entry.config.enabled,
                priority: entry.config.priority,
                status: entry.source.status(),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
