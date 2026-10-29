use serde::Serialize;
use std::fmt;

/// IPC 错误契约，字段与 docs/BACKEND_STRUCTURE.md 4.1 一致。
#[derive(Debug, Clone, Serialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

impl IpcError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn internal(detail: impl Into<String>) -> Self {
        Self::new("INTERNAL", "发生内部错误，请重试或查看日志").with_detail(detail)
    }

    pub fn window_not_found(label: &str) -> Self {
        Self::new("WINDOW_NOT_FOUND", format!("窗口 {label} 不存在"))
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for IpcError {}

pub type IpcResult<T> = Result<T, IpcError>;

/// 解析器错误。错误码与 BACKEND_STRUCTURE 4.1 的固定错误码对齐。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParserError {
    #[error("词典打开失败: {0}")]
    Open(String),
    #[error("词典查询失败: {0}")]
    Query(String),
    #[error("词典查询超时")]
    Timeout,
    #[error("词典繁忙")]
    Busy,
    #[error("词典 worker 已退出")]
    WorkerDead,
    #[error("词典数据无效: {0}")]
    InvalidData(String),
    #[error("资源不存在: {0}")]
    ResourceNotFound(String),
}

impl ParserError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Open(_) => "DICT_OPEN_FAILED",
            Self::Query(_) | Self::InvalidData(_) => "INTERNAL",
            Self::Timeout => "DICT_BUSY",
            Self::Busy => "DICT_BUSY",
            Self::WorkerDead => "DICT_NOT_READY",
            Self::ResourceNotFound(_) => "RESOURCE_NOT_FOUND",
        }
    }

    pub fn i18n_message(&self) -> String {
        self.to_string()
    }
}

/// 数据源错误。单源失败不应中断整体查询。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    #[error("数据源不可用: {0}")]
    Unavailable(String),
    #[error("数据源查询超时")]
    Timeout,
    #[error("数据源繁忙")]
    Busy,
    #[error("数据源内部错误: {0}")]
    Internal(String),
    #[error(transparent)]
    Parser(#[from] ParserError),
}

impl SourceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "DICT_NOT_READY",
            Self::Timeout | Self::Busy => "DICT_BUSY",
            Self::Internal(_) => "INTERNAL",
            Self::Parser(error) => error.code(),
        }
    }
}

/// 配置错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("读取配置失败: {0}")]
    Io(String),
    #[error("配置解析失败: {0}")]
    Parse(String),
    #[error("配置校验失败: {0}")]
    Validation(String),
    #[error("保存配置失败: {0}")]
    Save(String),
}

impl ConfigError {
    pub fn code(&self) -> &'static str {
        "CONFIG_INVALID"
    }
}

/// AnkiConnect 错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnkiError {
    #[error("Anki 未运行或 AnkiConnect 未安装")]
    Unreachable(String),
    #[error("AnkiConnect 返回错误: {0}")]
    Api(String),
    #[error("Anki 拒绝重复卡片")]
    Duplicate,
    #[error("Anki 模板缺少字段: {0}")]
    FieldMissing(String),
    #[error("Anki 请求无效: {0}")]
    InvalidData(String),
}

impl AnkiError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unreachable(_) => "ANKI_UNREACHABLE",
            Self::Api(_) => "ANKI_API_ERROR",
            Self::Duplicate => "ANKI_DUPLICATE",
            Self::FieldMissing(_) => "ANKI_FIELD_MISSING",
            Self::InvalidData(_) => "INTERNAL",
        }
    }
}

impl From<AnkiError> for IpcError {
    fn from(error: AnkiError) -> Self {
        IpcError::new(error.code(), error.to_string())
    }
}

impl From<ConfigError> for IpcError {
    fn from(error: ConfigError) -> Self {
        IpcError::new(error.code(), error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_error_codes_match_contract() {
        assert_eq!(ParserError::Open("x".into()).code(), "DICT_OPEN_FAILED");
        assert_eq!(ParserError::Timeout.code(), "DICT_BUSY");
        assert_eq!(ParserError::WorkerDead.code(), "DICT_NOT_READY");
        assert_eq!(
            ParserError::ResourceNotFound("a.png".into()).code(),
            "RESOURCE_NOT_FOUND"
        );
    }

    #[test]
    fn anki_error_codes_match_contract() {
        assert_eq!(AnkiError::Duplicate.code(), "ANKI_DUPLICATE");
        assert_eq!(
            AnkiError::FieldMissing("Word".into()).code(),
            "ANKI_FIELD_MISSING"
        );
    }

    #[test]
    fn source_error_maps_parser_code() {
        let error = SourceError::from(ParserError::Open("x".into()));
        assert_eq!(error.code(), "DICT_OPEN_FAILED");
    }
}
