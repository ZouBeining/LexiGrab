// SPDX-License-Identifier: AGPL-3.0-only
//! QuickJS 脚本响应解析器。契约见 docs/BACKEND_STRUCTURE.md 7.3。
//!
//! 每次查询新建 Runtime/Context，限制时间、内存与栈；不注入任何宿主能力。
//! 脚本必须定义全局 `parse({ responseBody, query })`，返回条目数组。

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use rquickjs::{CatchResultExt, CaughtError, Context, Function, Object, Runtime, Value};

use crate::core::entry::Entry;
use crate::core::error::ParserError;

use super::response::{finalize_entries, ParseContext, RawEntry, ResponseParser};

/// 脚本源码上限。
pub const MAX_SCRIPT_BYTES: usize = 64 * 1024;
/// 沙箱内存上限。
pub const SCRIPT_MEMORY_LIMIT: usize = 16 * 1024 * 1024;
/// 沙箱栈上限。
pub const SCRIPT_STACK_LIMIT: usize = 256 * 1024;
/// 默认截止时间。
pub const DEFAULT_TIMEOUT_MS: u64 = 200;
const MIN_TIMEOUT_MS: u64 = 50;
const MAX_TIMEOUT_MS: u64 = 1000;

#[derive(Default)]
struct Cache {
    source: Option<String>,
    mtime: Option<SystemTime>,
    len: u64,
}

/// 本地脚本解析器；源码按 mtime 缓存，脚本变更后自动重读。
pub struct ScriptParser {
    path: PathBuf,
    timeout: Duration,
    cache: Mutex<Cache>,
}

impl ScriptParser {
    /// 解析相对 `base_dir` 的脚本路径；`timeout_ms` 限定在 50–1000ms。
    pub fn new(path: PathBuf, base_dir: &Path, timeout_ms: Option<u64>) -> Self {
        let path = if path.is_absolute() {
            path
        } else {
            base_dir.join(path)
        };
        let timeout_ms = timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS);
        Self {
            path,
            timeout: Duration::from_millis(timeout_ms),
            cache: Mutex::new(Cache::default()),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn timeout_ms(&self) -> u64 {
        self.timeout.as_millis() as u64
    }

    /// 初始化时校验脚本文件存在、大小与编码；语法/运行错误在 `parse` 时报出。
    pub(crate) fn validate(&self) -> Result<(), ParserError> {
        self.load_source().map(|_| ())
    }

    /// 读取脚本源码，mtime 与长度未变时复用缓存。
    fn load_source(&self) -> Result<String, ParserError> {
        let metadata = std::fs::metadata(&self.path).map_err(|error| {
            ParserError::InvalidData(format!("读取脚本失败 {}: {error}", self.path.display()))
        })?;
        if !metadata.is_file() {
            return Err(ParserError::InvalidData(format!(
                "脚本路径不是文件: {}",
                self.path.display()
            )));
        }
        if metadata.len() > MAX_SCRIPT_BYTES as u64 {
            return Err(ParserError::InvalidData(format!(
                "脚本超过 {}KB 上限",
                MAX_SCRIPT_BYTES / 1024
            )));
        }
        let mtime = metadata.modified().ok();

        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| ParserError::InvalidData("脚本缓存锁已损坏".to_string()))?;
            if let Some(source) = &cache.source {
                if cache.mtime == mtime && cache.len == metadata.len() {
                    return Ok(source.clone());
                }
            }
        }

        let bytes = std::fs::read(&self.path).map_err(|error| {
            ParserError::InvalidData(format!("读取脚本失败 {}: {error}", self.path.display()))
        })?;
        if bytes.len() > MAX_SCRIPT_BYTES {
            return Err(ParserError::InvalidData(format!(
                "脚本超过 {}KB 上限",
                MAX_SCRIPT_BYTES / 1024
            )));
        }
        let source = String::from_utf8(bytes).map_err(|_| {
            ParserError::InvalidData(format!("脚本不是 UTF-8: {}", self.path.display()))
        })?;

        let mut cache = self
            .cache
            .lock()
            .map_err(|_| ParserError::InvalidData("脚本缓存锁已损坏".to_string()))?;
        *cache = Cache {
            source: Some(source.clone()),
            mtime,
            len: metadata.len(),
        };
        Ok(source)
    }
}

impl ResponseParser for ScriptParser {
    fn parse(&self, ctx: ParseContext<'_>) -> Result<Vec<Entry>, ParserError> {
        let source = self.load_source()?;
        let started = Instant::now();
        let deadline = started + self.timeout;

        let runtime = Runtime::new()
            .map_err(|error| ParserError::InvalidData(format!("创建脚本运行时失败: {error}")))?;
        runtime.set_memory_limit(SCRIPT_MEMORY_LIMIT);
        runtime.set_max_stack_size(SCRIPT_STACK_LIMIT);
        runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() >= deadline)));

        let context = Context::full(&runtime)
            .map_err(|error| ParserError::InvalidData(format!("创建脚本上下文失败: {error}")))?;
        let serialized = context.with(|js| -> Result<Option<String>, ParserError> {
            if let Err(error) = js.eval::<(), _>(source.as_bytes()).catch(&js) {
                return Err(classify(error, started, self.timeout));
            }

            let parse_fn: Function = match js.globals().get("parse").catch(&js) {
                Ok(function) => function,
                Err(error) => {
                    return Err(ParserError::InvalidData(format!(
                        "脚本必须定义 parse 函数: {error}"
                    )))
                }
            };

            let args = Object::new(js.clone())
                .map_err(|error| ParserError::InvalidData(format!("构造脚本参数失败: {error}")))?;
            args.set("responseBody", ctx.response_body)
                .and_then(|()| args.set("query", ctx.query))
                .map_err(|error| ParserError::InvalidData(format!("构造脚本参数失败: {error}")))?;

            let result: Value = match parse_fn.call((args,)).catch(&js) {
                Ok(value) => value,
                Err(error) => return Err(classify(error, started, self.timeout)),
            };

            if let Err(error) = js.globals().set("__lexigrab_result", result).catch(&js) {
                return Err(classify(error, started, self.timeout));
            }
            let json: Option<String> = match js.eval("JSON.stringify(__lexigrab_result)").catch(&js)
            {
                Ok(value) => value,
                Err(error) => return Err(classify(error, started, self.timeout)),
            };
            Ok(json)
        })?;

        let Some(serialized) = serialized else {
            return Err(ParserError::InvalidData(
                "脚本 parse 必须返回条目数组".to_string(),
            ));
        };
        let raw: Vec<RawEntry> = serde_json::from_str(&serialized)
            .map_err(|error| ParserError::InvalidData(format!("脚本返回结构无效: {error}")))?;
        finalize_entries(raw, ctx.query)
    }
}

/// 把 QuickJS 错误分类为解析器错误：内存/超时/其他运行错误。
fn classify(error: CaughtError<'_>, started: Instant, timeout: Duration) -> ParserError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let out_of_memory = matches!(&error, CaughtError::Error(rquickjs::Error::Allocation))
        || lower.contains("out of memory")
        || lower.contains("memory limit");
    if out_of_memory {
        return ParserError::InvalidData(format!("脚本内存超限: {message}"));
    }
    if started.elapsed() >= timeout || lower.contains("interrupted") {
        return ParserError::Timeout;
    }
    ParserError::InvalidData(format!("脚本执行失败: {message}"))
}
