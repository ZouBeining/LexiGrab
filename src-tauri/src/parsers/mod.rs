//! 词典格式解析器。

pub mod jsonpath;
pub mod mdx;
pub mod response;
pub mod script;
pub mod sense;

use std::path::Path;

use crate::core::entry::Entry;
use crate::core::error::ParserError;

/// 解析器契约，见 docs/BACKEND_STRUCTURE.md 7.1。
pub trait FormatParser: Send + Sync {
    fn can_parse(&self, path: &Path) -> bool;
    fn open(&mut self, path: &Path) -> Result<(), ParserError>;
    fn lookup(&self, query: &str) -> Result<Vec<Entry>, ParserError>;
}

/// MDX/MDD 资源读取。资源在查询后按需读取，不随查询返回字节。
pub trait ResourceProvider: Send + Sync {
    fn resolve(&self, path: &str) -> Result<ResourceData, ParserError>;
}

/// 资源字节与 MIME。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceData {
    pub bytes: Vec<u8>,
    pub mime: String,
}

impl ResourceData {
    pub fn new(bytes: Vec<u8>, mime: impl Into<String>) -> Self {
        Self {
            bytes,
            mime: mime.into(),
        }
    }
}
