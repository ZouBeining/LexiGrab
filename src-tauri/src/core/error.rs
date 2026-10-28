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
