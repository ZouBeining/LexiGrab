//! 远程 HTTP 数据源。契约见 docs/BACKEND_STRUCTURE.md 2.2、7.2、7.3 与 PLAN_M4 T4.1。
//!
//! 查询是同步阻塞的；调用方负责放到 blocking 线程。远程失败只影响本源。

use std::io::Read;
use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderName, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::redirect::Policy;
use url::Url;

use crate::core::entry::{Entry, ResponseParserConfig, SourceConfig, SourceKind};
use crate::core::error::{ParserError, SourceError};
use crate::parsers::jsonpath::JsonPathParser;
use crate::parsers::response::{ParseContext, ResponseParser, MAX_ENTRIES};
use crate::parsers::script::ScriptParser;
use crate::sources::{DictionarySource, SourceResource, SourceStatus};

/// 响应体上限：2MB。
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const CONNECT_TIMEOUT_MS: u64 = 1500;
const RETRY_DELAY_MS: u64 = 150;
const MAX_REDIRECTS: usize = 2;

/// 查询词按 UTF-8 百分号编码；保留 RFC 3986 unreserved 字符。
const QUERY_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// 获取失败，区分可重试与不可重试；预览为已读到的响应片段。
enum FetchFailure {
    Retry(SourceError, Option<String>),
    Fatal(SourceError, Option<String>),
}

/// 草稿配置的测试结果：条目 + 原始响应前 4KB（失败时也尽量返回）。
#[derive(Debug)]
pub struct RemoteTestOutcome {
    pub entries: Vec<Entry>,
    pub raw_preview: Option<String>,
}

/// 测试查询失败：命令层可同时展示错误码与原始响应预览。
#[derive(Debug)]
pub struct RemoteTestError {
    pub error: SourceError,
    pub raw_preview: Option<String>,
}

/// 一个 `SourceConfig` 对应的远程 HTTP 数据源。
pub struct RemoteHttpSource {
    config: SourceConfig,
    url_template: String,
    parser: Option<Box<dyn ResponseParser>>,
    auth: Option<(HeaderName, HeaderValue)>,
    client: Option<Client>,
    init_error: Option<String>,
    /// 最近一次查询失败；成功查询后清空，供设置页显示“上次失败”。
    last_error: Mutex<Option<String>>,
}

impl RemoteHttpSource {
    /// 永不 panic：模板/鉴权/脚本/客户端初始化错误记录为 `init_error`。
    pub fn open(config: SourceConfig, base_dir: &Path) -> Self {
        match Self::build(&config, base_dir) {
            Ok((url_template, parser, auth, client)) => Self {
                config,
                url_template,
                parser: Some(parser),
                auth,
                client: Some(client),
                init_error: None,
                last_error: Mutex::new(None),
            },
            Err(message) => {
                tracing::warn!(
                    source_id = %config.id,
                    source_name = %config.name,
                    "远程数据源配置无效，已标记失败"
                );
                Self {
                    config,
                    url_template: String::new(),
                    parser: None,
                    auth: None,
                    client: None,
                    init_error: Some(message),
                    last_error: Mutex::new(None),
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn build(
        config: &SourceConfig,
        base_dir: &Path,
    ) -> Result<
        (
            String,
            Box<dyn ResponseParser>,
            Option<(HeaderName, HeaderValue)>,
            Client,
        ),
        String,
    > {
        let SourceKind::RemoteHttp {
            url_template,
            response_parser,
            auth_header,
            auth_value,
            timeout_ms,
        } = &config.kind
        else {
            return Err("不是远程 HTTP 数据源".to_string());
        };

        validate_template(url_template)?;

        let parser: Box<dyn ResponseParser> = match response_parser {
            ResponseParserConfig::Jsonpath { expression } => Box::new(
                JsonPathParser::new(expression.clone()).map_err(|error| error.to_string())?,
            ),
            ResponseParserConfig::Script { path, timeout_ms } => {
                let parser = ScriptParser::new(path.clone(), base_dir, *timeout_ms);
                parser.validate().map_err(|error| error.to_string())?;
                Box::new(parser)
            }
        };

        let auth = auth_headers(auth_header.as_deref(), auth_value.as_deref())?;
        let timeout = Duration::from_millis((*timeout_ms).clamp(200, 15_000));
        let client = Client::builder()
            .connect_timeout(Duration::from_millis(CONNECT_TIMEOUT_MS))
            .timeout(timeout)
            .redirect(Policy::custom(redirect_policy))
            .build()
            .map_err(|error| format!("创建 HTTP 客户端失败: {error}"))?;

        Ok((url_template.clone(), parser, auth, client))
    }

    fn request_url(&self, query: &str) -> String {
        let encoded = utf8_percent_encode(query, QUERY_ENCODE_SET).to_string();
        self.url_template.replace("{word}", &encoded)
    }

    /// 覆盖“上次失败”状态；锁中毒时忽略，不让状态写入影响查询。
    fn set_last_error(&self, message: Option<String>) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = message;
        }
    }

    /// 连接失败/超时重试一次，其余失败直接返回；4xx/5xx 不重试。
    fn fetch(&self, client: &Client, url: &str) -> Result<String, (SourceError, Option<String>)> {
        let mut last_error = None;
        for attempt in 0..=1 {
            if attempt > 0 {
                thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
            }
            match self.fetch_once(client, url) {
                Ok(body) => return Ok(body),
                Err(FetchFailure::Fatal(error, preview)) => return Err((error, preview)),
                Err(FetchFailure::Retry(error, preview)) => last_error = Some((error, preview)),
            }
        }
        Err(last_error
            .unwrap_or_else(|| (SourceError::Unavailable("远程请求失败".to_string()), None)))
    }

    fn fetch_once(&self, client: &Client, url: &str) -> Result<String, FetchFailure> {
        let mut request = client.get(url).header(ACCEPT, "application/json").header(
            USER_AGENT,
            format!("LexiGrab/{}", env!("CARGO_PKG_VERSION")),
        );
        if let Some((name, value)) = &self.auth {
            request = request.header(name.clone(), value.clone());
        }

        let mut response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                let retryable = error.is_timeout() || error.is_connect();
                let mapped = if error.is_timeout() {
                    SourceError::Timeout
                } else {
                    SourceError::Unavailable(format!("连接远程服务失败: {error}"))
                };
                return Err(if retryable {
                    FetchFailure::Retry(mapped, None)
                } else {
                    FetchFailure::Fatal(mapped, None)
                });
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Err(FetchFailure::Fatal(
                SourceError::Unavailable(format!("远程服务返回 HTTP {}", status.as_u16())),
                read_preview(&mut response),
            ));
        }

        let body = read_body(&mut response)?;
        String::from_utf8(body).map_err(|_| {
            FetchFailure::Fatal(
                SourceError::Parser(ParserError::InvalidData("响应体不是 UTF-8".to_string())),
                None,
            )
        })
    }
}

/// 读取响应体并对 2MB 上限做流式检查。
fn read_body(response: &mut Response) -> Result<Vec<u8>, FetchFailure> {
    let mut body = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match response.read(&mut buffer) {
            Ok(0) => return Ok(body),
            Ok(read) => {
                body.extend_from_slice(&buffer[..read]);
                if body.len() > MAX_RESPONSE_BYTES {
                    return Err(FetchFailure::Fatal(
                        SourceError::Unavailable(format!(
                            "响应体超过 {}MB",
                            MAX_RESPONSE_BYTES / (1024 * 1024)
                        )),
                        Some(preview_bytes(&body)),
                    ));
                }
            }
            Err(error) => {
                let mapped = if error.kind() == std::io::ErrorKind::TimedOut {
                    SourceError::Timeout
                } else {
                    SourceError::Unavailable(format!("读取响应失败: {error}"))
                };
                return Err(FetchFailure::Retry(mapped, Some(preview_bytes(&body))));
            }
        }
    }
}

/// 读取响应前 4KB 作为预览；读取失败不覆盖原始 HTTP 错误。
fn read_preview(response: &mut Response) -> Option<String> {
    let mut body = Vec::new();
    let mut buffer = [0u8; 4096];
    if let Ok(read) = response.read(&mut buffer) {
        body.extend_from_slice(&buffer[..read]);
    }
    (!body.is_empty()).then(|| preview_bytes(&body))
}

/// 按字符截断，避免从 UTF-8 多字节序列中间切断。
fn preview_bytes(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    preview_text(&text)
}

fn preview_text(text: &str) -> String {
    text.chars().take(4096).collect()
}

/// 草稿配置测试：不写缓存、不写配置，复用同样的请求与解析路径。
pub fn test_query(
    config: &SourceConfig,
    base_dir: &Path,
    query: &str,
) -> Result<RemoteTestOutcome, RemoteTestError> {
    if !matches!(&config.kind, SourceKind::RemoteHttp { .. }) {
        return Err(RemoteTestError {
            error: SourceError::Unavailable("不是远程 HTTP 数据源".to_string()),
            raw_preview: None,
        });
    }

    let source = RemoteHttpSource::open(config.clone(), base_dir);
    if let Some(message) = &source.init_error {
        return Err(RemoteTestError {
            error: SourceError::Unavailable(message.clone()),
            raw_preview: None,
        });
    }
    let (Some(client), Some(parser)) = (&source.client, &source.parser) else {
        return Err(RemoteTestError {
            error: SourceError::Unavailable("远程数据源未初始化".to_string()),
            raw_preview: None,
        });
    };

    let url = source.request_url(query);
    let body = match source.fetch(client, &url) {
        Ok(body) => body,
        Err((error, raw_preview)) => return Err(RemoteTestError { error, raw_preview }),
    };
    let raw_preview = Some(preview_text(&body));
    match parser.parse(ParseContext {
        response_body: &body,
        query,
    }) {
        Ok(mut entries) => {
            entries.truncate(MAX_ENTRIES);
            for entry in &mut entries {
                entry.source_name = config.name.clone();
            }
            Ok(RemoteTestOutcome {
                entries,
                raw_preview,
            })
        }
        Err(error) => Err(RemoteTestError {
            error: SourceError::Parser(error),
            raw_preview,
        }),
    }
}

impl DictionarySource for RemoteHttpSource {
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

    fn is_remote(&self) -> bool {
        true
    }

    fn status(&self) -> SourceStatus {
        if let Some(message) = &self.init_error {
            return SourceStatus::Failed {
                message: message.clone(),
            };
        }
        match self.last_error.lock() {
            Ok(slot) => match slot.as_ref() {
                Some(message) => SourceStatus::Failed {
                    message: message.clone(),
                },
                None => SourceStatus::Ready { entry_count: 0 },
            },
            Err(_) => SourceStatus::Failed {
                message: "数据源状态不可读".to_string(),
            },
        }
    }

    fn lookup(&self, query: &str) -> Result<Vec<Entry>, SourceError> {
        if let Some(message) = &self.init_error {
            return Err(SourceError::Unavailable(message.clone()));
        }
        let result = self.lookup_inner(query);
        match &result {
            Ok(_) => self.set_last_error(None),
            Err(error) => self.set_last_error(Some(error.to_string())),
        }
        result
    }

    fn resolve_resource(&self, _dict_id: &str, _path: &str) -> Result<SourceResource, SourceError> {
        Err(SourceError::Unavailable(
            "远程数据源不提供本地资源".to_string(),
        ))
    }
}

impl RemoteHttpSource {
    fn lookup_inner(&self, query: &str) -> Result<Vec<Entry>, SourceError> {
        let (Some(client), Some(parser)) = (&self.client, &self.parser) else {
            return Err(SourceError::Unavailable("远程数据源未初始化".to_string()));
        };

        let url = self.request_url(query);
        let body = self
            .fetch(client, &url)
            .map_err(|(error, _preview)| error)?;
        let mut entries = parser
            .parse(ParseContext {
                response_body: &body,
                query,
            })
            .map_err(SourceError::Parser)?;
        entries.truncate(MAX_ENTRIES);
        for entry in &mut entries {
            entry.source_name = self.config.name.clone();
        }
        Ok(entries)
    }
}

/// 模板必须恰好一个 `{word}`，且替换后是带主机的 http/https URL。
fn validate_template(template: &str) -> Result<(), String> {
    if template.matches("{word}").count() != 1 {
        return Err("URL 模板必须恰好包含一个 {word}".to_string());
    }
    let probe = template.replace("{word}", "lexigrab");
    let parsed = Url::parse(&probe).map_err(|error| format!("URL 模板无效: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("URL 模板只允许 http/https".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("URL 模板缺少主机".to_string());
    }
    Ok(())
}

/// 鉴权头要么都不配，要么都配置且合法；错误信息不回显值。
fn auth_headers(
    header: Option<&str>,
    value: Option<&str>,
) -> Result<Option<(HeaderName, HeaderValue)>, String> {
    match (header, value) {
        (None, None) => Ok(None),
        (Some(header), Some(value)) => {
            let header = header.trim();
            let value = value.trim();
            if header.is_empty() || value.is_empty() {
                return Err("鉴权头名称和值不能为空".to_string());
            }
            if header.contains('\r')
                || header.contains('\n')
                || value.contains('\r')
                || value.contains('\n')
            {
                return Err("鉴权头名称和值不能包含换行".to_string());
            }
            let name = HeaderName::from_bytes(header.as_bytes())
                .map_err(|_| "鉴权头名称无效".to_string())?;
            let value =
                HeaderValue::from_str(value).map_err(|_| "鉴权头值包含非法字符".to_string())?;
            Ok(Some((name, value)))
        }
        _ => Err("鉴权头名称和值必须同时配置".to_string()),
    }
}

/// 跨主机停止；同主机最多跟随 2 次。
fn redirect_policy(attempt: reqwest::redirect::Attempt<'_>) -> reqwest::redirect::Action {
    let Some(previous) = attempt.previous().last() else {
        return attempt.follow();
    };
    if !same_host(previous, attempt.url()) || attempt.previous().len() >= MAX_REDIRECTS {
        return attempt.stop();
    }
    attempt.follow()
}

fn same_host(left: &Url, right: &Url) -> bool {
    match (left.host_str(), right.host_str()) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => false,
    }
}
