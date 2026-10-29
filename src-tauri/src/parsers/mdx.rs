//! MDX 解析器适配层：每个文件一个 worker 线程，外部只保留命令通道。
//!
//! 依赖 `mdictlib`（MIT）。原计划中的 libmdx git 依赖在当前网络不可达，
//! 选型记录见 .agents/notes/M1-core-baseline.md 与 PROGRESS.md。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use mdictlib::{MddFile, MdxFile};
use unicode_normalization::UnicodeNormalization;

use crate::core::entry::Entry;
use crate::core::error::ParserError;
use crate::core::html;
use crate::parsers::{FormatParser, ResourceData, ResourceProvider};

/// 词典打开超时。
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(15);
/// 单次查询/资源读取超时。
pub const QUERY_TIMEOUT: Duration = Duration::from_millis(500);
/// 关闭时等待 worker 退出的上限。
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
/// 命令队列容量，满时返回 DICT_BUSY。
const QUEUE_CAPACITY: usize = 8;
/// 连续超时多少次标记 unhealthy。
const MAX_CONSECUTIVE_TIMEOUTS: u32 = 3;
/// 单次查询最多返回条目数。
const MAX_ENTRIES_PER_QUERY: usize = 3;

enum WorkerCommand {
    Lookup {
        query: String,
        reply: SyncSender<Result<Vec<Entry>, ParserError>>,
    },
    Resource {
        path: String,
        reply: SyncSender<Result<ResourceData, ParserError>>,
    },
    Shutdown,
}

struct Worker {
    tx: mpsc::SyncSender<WorkerCommand>,
    done: Mutex<Receiver<()>>,
    join: Option<JoinHandle<()>>,
}

/// MDX 文件的解析器句柄。
pub struct MdxParser {
    dict_id: String,
    entry_count: u64,
    worker: Option<Worker>,
    consecutive_timeouts: AtomicU32,
    unhealthy: AtomicBool,
    /// worker 退出后需要回收线程；`&self` 方法里用 Mutex 保证只回收一次。
    detached: Mutex<bool>,
}

impl MdxParser {
    pub fn new(dict_id: impl Into<String>) -> Self {
        Self {
            dict_id: dict_id.into(),
            entry_count: 0,
            worker: None,
            consecutive_timeouts: AtomicU32::new(0),
            unhealthy: AtomicBool::new(false),
            detached: Mutex::new(false),
        }
    }

    pub fn dict_id(&self) -> &str {
        &self.dict_id
    }

    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    pub fn is_unhealthy(&self) -> bool {
        self.unhealthy.load(Ordering::SeqCst)
    }

    /// 主动关闭 worker；之后的查询返回 `WorkerDead` 而不是 panic。
    pub fn close(&mut self) {
        self.shutdown();
    }

    fn ensure_worker(&self) -> Result<&Worker, ParserError> {
        if self.unhealthy.load(Ordering::SeqCst) {
            return Err(ParserError::WorkerDead);
        }
        self.worker.as_ref().ok_or(ParserError::WorkerDead)
    }

    fn shutdown(&mut self) {
        let Some(mut worker) = self.worker.take() else {
            return;
        };
        let _ = worker.tx.send(WorkerCommand::Shutdown);
        let wait = worker
            .done
            .lock()
            .map(|done| done.recv_timeout(SHUTDOWN_TIMEOUT).is_ok())
            .unwrap_or(false);
        if !wait {
            // 超时 detach：线程可能卡在 libmdx 内部，不阻塞调用方。
            if let Ok(mut detached) = self.detached.lock() {
                *detached = true;
            }
            return;
        }
        if let Some(join) = worker.join.take() {
            let _ = join.join();
        }
    }

    fn lookup_variants(query: &str) -> Vec<String> {
        let normalized = normalize_query(query);
        if normalized.is_empty() {
            return Vec::new();
        }
        let mut variants = Vec::with_capacity(4);
        push_unique(&mut variants, normalized.clone());
        push_unique(&mut variants, normalized.to_lowercase());
        push_unique(&mut variants, normalized.to_uppercase());
        push_unique(&mut variants, capitalize_first(&normalized));
        variants
    }

    fn send_lookup(&self, query: String) -> Result<Vec<Entry>, ParserError> {
        let worker = self.ensure_worker()?;
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        worker
            .tx
            .try_send(WorkerCommand::Lookup {
                query,
                reply: reply_tx,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => ParserError::Busy,
                mpsc::TrySendError::Disconnected(_) => ParserError::WorkerDead,
            })?;
        match reply_rx.recv_timeout(QUERY_TIMEOUT) {
            Ok(result) => {
                self.consecutive_timeouts.store(0, Ordering::SeqCst);
                result
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let previous = self.consecutive_timeouts.fetch_add(1, Ordering::SeqCst);
                if previous + 1 >= MAX_CONSECUTIVE_TIMEOUTS {
                    self.unhealthy.store(true, Ordering::SeqCst);
                }
                Err(ParserError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.unhealthy.store(true, Ordering::SeqCst);
                Err(ParserError::WorkerDead)
            }
        }
    }

    fn send_resource(&self, path: String) -> Result<ResourceData, ParserError> {
        let worker = self.ensure_worker()?;
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        worker
            .tx
            .try_send(WorkerCommand::Resource {
                path,
                reply: reply_tx,
            })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => ParserError::Busy,
                mpsc::TrySendError::Disconnected(_) => ParserError::WorkerDead,
            })?;
        match reply_rx.recv_timeout(QUERY_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ParserError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.unhealthy.store(true, Ordering::SeqCst);
                Err(ParserError::WorkerDead)
            }
        }
    }
}

impl Drop for MdxParser {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl FormatParser for MdxParser {
    fn can_parse(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.eq_ignore_ascii_case("mdx"))
            .unwrap_or(false)
    }

    fn open(&mut self, path: &Path) -> Result<(), ParserError> {
        self.open_with_timeout(path, OPEN_TIMEOUT)
    }

    fn lookup(&self, query: &str) -> Result<Vec<Entry>, ParserError> {
        let variants = Self::lookup_variants(query);
        if variants.is_empty() {
            return Ok(Vec::new());
        }
        self.send_lookup(variants.join("\u{1f}"))
    }
}

impl MdxParser {
    /// 与 `FormatParser::open` 相同，但允许调用方指定超时（CLI/测试用）。
    pub fn open_with_timeout(&mut self, path: &Path, timeout: Duration) -> Result<(), ParserError> {
        self.shutdown();
        let path = path.to_path_buf();
        let dict_id = self.dict_id.clone();
        let (open_tx, open_rx) = mpsc::sync_channel(1);
        let (tx, rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (done_tx, done_rx) = mpsc::channel();
        let join = thread::Builder::new()
            .name(format!("mdx-{dict_id}"))
            .spawn(move || {
                run_worker(path, dict_id, rx, open_tx);
                let _ = done_tx.send(());
            })
            .map_err(|error| ParserError::Open(error.to_string()))?;

        match open_rx.recv_timeout(timeout) {
            Ok(Ok(entry_count)) => {
                self.entry_count = entry_count;
                self.worker = Some(Worker {
                    tx,
                    done: Mutex::new(done_rx),
                    join: Some(join),
                });
                self.consecutive_timeouts.store(0, Ordering::SeqCst);
                self.unhealthy.store(false, Ordering::SeqCst);
                Ok(())
            }
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => Err(ParserError::Open("打开词典超时".to_string())),
        }
    }
}

impl ResourceProvider for MdxParser {
    fn resolve(&self, path: &str) -> Result<ResourceData, ParserError> {
        self.send_resource(path.to_string())
    }
}

fn run_worker(
    path: PathBuf,
    dict_id: String,
    rx: Receiver<WorkerCommand>,
    open_tx: SyncSender<Result<u64, ParserError>>,
) {
    let file = match MdxFile::open(&path) {
        Ok(file) => file,
        Err(error) => {
            let _ = open_tx.send(Err(ParserError::Open(error.to_string())));
            return;
        }
    };
    let entry_count = file.len();
    if open_tx.send(Ok(entry_count)).is_err() {
        return;
    }
    let mdd_path = sibling_mdd_path(&path);
    let mut resources: Option<Option<MddFile>> = None;

    while let Ok(command) = rx.recv() {
        match command {
            WorkerCommand::Shutdown => break,
            WorkerCommand::Lookup { query, reply } => {
                let _ = reply.send(lookup_entries(&file, &query, &dict_id));
            }
            WorkerCommand::Resource { path, reply } => {
                if resources.is_none() {
                    resources = Some(MddFile::open(&mdd_path).ok());
                }
                let result =
                    read_resource(resources.as_ref().and_then(|open| open.as_ref()), &path);
                let _ = reply.send(result);
            }
        }
    }
}

fn lookup_entries(
    file: &MdxFile,
    joined_variants: &str,
    dict_id: &str,
) -> Result<Vec<Entry>, ParserError> {
    let mut entries = Vec::new();
    let mut seen = Vec::new();
    for query in joined_variants.split('\u{1f}') {
        if entries.len() >= MAX_ENTRIES_PER_QUERY {
            break;
        }
        let record = match file.lookup(query) {
            Ok(Some(record)) => record,
            Ok(None) => continue,
            Err(error) => return Err(ParserError::Query(error.to_string())),
        };
        let ordinal = record.ordinal().get();
        if seen.contains(&ordinal) {
            continue;
        }
        seen.push(ordinal);
        let raw_html = record.text().to_string();
        let headword = record.key().to_string();
        match html::rewrite_and_sanitize(&raw_html, dict_id) {
            Ok(rewritten) => entries.push(Entry {
                headword,
                definition_html: rewritten.html,
                phonetic: None,
                resources: rewritten.resources,
                source_name: String::new(),
            }),
            Err(error) => {
                tracing::warn!(
                    code = error.code(),
                    headword = %headword,
                    "跳过无效的词典 HTML"
                );
            }
        }
    }
    Ok(entries)
}

fn read_resource(resources: Option<&MddFile>, path: &str) -> Result<ResourceData, ParserError> {
    let normalized = normalize_resource_path(path)?;
    let file = resources.ok_or_else(|| {
        ParserError::ResourceNotFound(format!("未找到同名词典资源文件: {normalized}"))
    })?;
    let resource = file
        .lookup(&normalized)
        .map_err(|error| ParserError::Query(error.to_string()))?
        .ok_or_else(|| ParserError::ResourceNotFound(normalized.clone()))?;
    let mime = mime_guess::from_path(&normalized)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    Ok(ResourceData::new(resource.bytes().to_vec(), mime))
}

/// 接受正斜杠与反斜杠回退，拒绝 `..`。
pub fn normalize_resource_path(path: &str) -> Result<String, ParserError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(ParserError::ResourceNotFound("空资源路径".to_string()));
    }
    let segments: Vec<&str> = trimmed
        .split(['/', '\\'])
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.contains(&"..") {
        return Err(ParserError::ResourceNotFound(trimmed.to_string()));
    }
    if segments.is_empty() {
        return Err(ParserError::ResourceNotFound(trimmed.to_string()));
    }
    Ok(segments.join("/"))
}

/// 查询归一化：trim + NFKC + 去首尾标点。
pub fn normalize_query(query: &str) -> String {
    let nfkc: String = query.nfkc().collect();
    let trimmed = nfkc.trim_matches(|character: char| {
        character.is_whitespace() || is_trimmed_punctuation(character)
    });
    trimmed.trim().to_string()
}

fn is_trimmed_punctuation(character: char) -> bool {
    character.is_ascii_punctuation()
        || matches!(
            character,
            '“' | '”'
                | '‘'
                | '’'
                | '「'
                | '」'
                | '『'
                | '』'
                | '（'
                | '）'
                | '《'
                | '》'
                | '【'
                | '】'
                | '、'
                | '。'
                | '，'
                | '？'
                | '！'
                | '；'
                | '：'
                | '…'
        )
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.contains(&value) {
        values.push(value);
    }
}

fn capitalize_first(value: &str) -> String {
    let mut characters = value.chars();
    match characters.next() {
        Some(first) => {
            let mut result = String::with_capacity(value.len());
            result.extend(first.to_uppercase());
            result.push_str(characters.as_str());
            result
        }
        None => String::new(),
    }
}

/// MDD 文件与 MDX 同目录同名。
pub fn sibling_mdd_path(mdx: &Path) -> PathBuf {
    mdx.with_extension("mdd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_query_trims_punctuation_and_nfkc() {
        assert_eq!(normalize_query("  “hello”  "), "hello");
        assert_eq!(normalize_query("ＨＥＬＬＯ"), "HELLO");
        assert_eq!(normalize_query("..."), "");
    }

    #[test]
    fn lookup_variants_cover_case_forms() {
        let variants = MdxParser::lookup_variants("Hello");
        assert_eq!(variants, vec!["Hello", "hello", "HELLO"]);
        let variants = MdxParser::lookup_variants("hello");
        assert_eq!(variants, vec!["hello", "HELLO", "Hello"]);
    }

    #[test]
    fn resource_paths_accept_backslash_and_reject_parent() {
        assert_eq!(
            normalize_resource_path("\\img\\a.png").expect("normalized"),
            "img/a.png"
        );
        assert!(normalize_resource_path("../secret").is_err());
        assert!(normalize_resource_path("").is_err());
    }

    #[test]
    fn sibling_mdd_uses_mdd_extension() {
        assert_eq!(
            sibling_mdd_path(Path::new("dicts/mini.mdx")),
            PathBuf::from("dicts/mini.mdd")
        );
    }

    #[test]
    fn can_parse_only_mdx() {
        let parser = MdxParser::new("local-test");
        assert!(parser.can_parse(Path::new("a.mdx")));
        assert!(parser.can_parse(Path::new("a.MDX")));
        assert!(!parser.can_parse(Path::new("a.mdd")));
    }
}
