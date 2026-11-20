//! 数据源抽象。契约见 docs/BACKEND_STRUCTURE.md 2.2 与 7.

pub mod local_file;
pub mod remote_http;

use serde::Serialize;

use crate::core::entry::{Entry, SourceConfig, SourceKind};
use crate::core::error::SourceError;

use std::path::Path;
use std::sync::Arc;

/// 数据源运行时状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SourceStatus {
    Loading,
    Ready { entry_count: u64 },
    Failed { message: String },
}

impl SourceStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }
}

/// 资源字节与 MIME，由数据源层返回给 M2 的 `mdx-res` 协议。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceResource {
    pub bytes: Vec<u8>,
    pub mime: String,
}

/// 数据源契约。查询是同步阻塞的，调用方负责放到 blocking 线程。
pub trait DictionarySource: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn priority(&self) -> i32;
    fn enabled(&self) -> bool;
    fn status(&self) -> SourceStatus;

    /// 查询本源的条目；`source_name` 由实现填写。
    fn lookup(&self, query: &str) -> Result<Vec<Entry>, SourceError>;

    /// 按逻辑 dict_id + 相对路径读取资源。
    fn resolve_resource(&self, dict_id: &str, path: &str) -> Result<SourceResource, SourceError>;

    /// 是否为远程 HTTP 源；路由据此决定同步等待还是后台追加。
    fn is_remote(&self) -> bool {
        false
    }
}

/// 按配置构造数据源；配置错误由各数据源记录为 Failed 状态，不 panic。
pub fn build_source(config: SourceConfig, base_dir: &Path) -> Arc<dyn DictionarySource> {
    match &config.kind {
        SourceKind::LocalFile { .. } => {
            Arc::new(local_file::LocalFileSource::open(config, base_dir))
        }
        SourceKind::RemoteHttp { .. } => {
            Arc::new(remote_http::RemoteHttpSource::open(config, base_dir))
        }
    }
}
