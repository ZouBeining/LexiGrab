//! 本地 MDX 文件数据源：每个文件一个 worker，查询失败只影响该文件。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::core::entry::{Entry, SourceConfig, SourceKind};
use crate::core::error::SourceError;
use crate::parsers::mdx::MdxParser;
use crate::parsers::{FormatParser, ResourceProvider};
use crate::sources::{DictionarySource, SourceResource, SourceStatus};

struct LocalFile {
    dict_id: String,
    path: PathBuf,
    parser: MdxParser,
}

/// 一个 `SourceConfig` 对应一个数据源，可包含多个 MDX 文件。
pub struct LocalFileSource {
    config: SourceConfig,
    files: Vec<LocalFile>,
    status: RwLock<SourceStatus>,
}

impl LocalFileSource {
    /// 打开配置中的全部文件；部分失败不阻塞其他文件。
    pub fn open(config: SourceConfig, base_dir: &Path) -> Self {
        let source_id = config.id.clone();
        let source_name = config.name.clone();
        let paths = match &config.kind {
            SourceKind::LocalFile { paths } => paths.clone(),
            SourceKind::RemoteHttp { .. } => Vec::new(),
        };

        let mut files = Vec::new();
        let mut errors = Vec::new();
        let mut entry_count = 0u64;
        for (index, relative) in paths.iter().enumerate() {
            let path = if relative.is_absolute() {
                relative.clone()
            } else {
                base_dir.join(relative)
            };
            let dict_id = format!("{source_id}-f{}", index + 1);
            let mut parser = MdxParser::new(dict_id.clone());
            match parser.open(&path) {
                Ok(()) => {
                    entry_count += parser.entry_count();
                    files.push(LocalFile {
                        dict_id,
                        path,
                        parser,
                    });
                }
                Err(error) => {
                    tracing::warn!(
                        code = error.code(),
                        source_id = %source_id,
                        source_name = %source_name,
                        path = %path.display(),
                        "打开词典失败，跳过该文件"
                    );
                    errors.push(format!("{}: {error}", path.display()));
                }
            }
        }

        let status = if files.is_empty() && !errors.is_empty() {
            SourceStatus::Failed {
                message: errors.join("；"),
            }
        } else {
            SourceStatus::Ready { entry_count }
        };

        Self {
            config,
            files,
            status: RwLock::new(status),
        }
    }

    pub fn files(&self) -> usize {
        self.files.len()
    }

    /// 已打开文件的逻辑 dict_id，供 M2 的 `mdx-res` 协议使用。
    pub fn dict_ids(&self) -> Vec<String> {
        self.files.iter().map(|file| file.dict_id.clone()).collect()
    }

    fn file(&self, dict_id: &str) -> Option<&LocalFile> {
        self.files.iter().find(|file| file.dict_id == dict_id)
    }

    fn query_file(file: &LocalFile, query: &str) -> Result<Vec<Entry>, SourceError> {
        file.parser.lookup(query).map_err(SourceError::from)
    }
}

impl DictionarySource for LocalFileSource {
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
        self.config.enabled
    }

    fn status(&self) -> SourceStatus {
        self.status
            .read()
            .map(|status| status.clone())
            .unwrap_or(SourceStatus::Failed {
                message: "数据源状态不可读".to_string(),
            })
    }

    fn lookup(&self, query: &str) -> Result<Vec<Entry>, SourceError> {
        if self.files.is_empty() {
            return Err(SourceError::Unavailable(match self.status() {
                SourceStatus::Failed { message } => message,
                _ => "没有可用的词典文件".to_string(),
            }));
        }
        let mut entries = Vec::new();
        let mut last_error: Option<SourceError> = None;
        let mut succeeded = false;
        for file in &self.files {
            match Self::query_file(file, query) {
                Ok(mut found) => {
                    succeeded = true;
                    for entry in &mut found {
                        entry.source_name = self.config.name.clone();
                    }
                    entries.extend(found);
                }
                Err(error) => {
                    tracing::warn!(
                        code = error.code(),
                        source_id = %self.config.id,
                        path = %file.path.display(),
                        "词典文件查询失败"
                    );
                    last_error = Some(error);
                }
            }
        }
        if !succeeded {
            return Err(last_error.unwrap_or(SourceError::Internal("没有词典文件".to_string())));
        }
        Ok(entries)
    }

    fn resolve_resource(&self, dict_id: &str, path: &str) -> Result<SourceResource, SourceError> {
        let file = self
            .file(dict_id)
            .ok_or_else(|| SourceError::Internal(format!("未知词典 {dict_id}")))?;
        let data = file.parser.resolve(path).map_err(SourceError::from)?;
        Ok(SourceResource {
            bytes: data.bytes,
            mime: data.mime,
        })
    }
}
