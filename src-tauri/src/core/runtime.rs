use std::sync::Mutex;

/// 查询 id，M1 由 CLI/测试生成；M2 由 IPC 命令生成。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueryId(String);

impl QueryId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn from_string(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for QueryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for QueryId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// 运行时事件。M1 只用于记录源失败；M2 由 Tauri 事件层转发；M4 增加远程结果。
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeEvent {
    SourceFailure {
        query_id: String,
        source_id: String,
        source_name: String,
        code: String,
        message: String,
    },
    SourceSlow {
        query_id: String,
        source_id: String,
        elapsed_ms: u64,
    },
    /// 远程源异步返回（含缓存命中）；`cached` 表示来自本地缓存。
    RemoteResult {
        query_id: String,
        source_id: String,
        source_name: String,
        entries: Vec<crate::core::entry::Entry>,
        cached: bool,
        elapsed_ms: u64,
    },
    /// 远程源失败；只做提示，不影响已返回的本地结果。
    RemoteError {
        query_id: String,
        source_id: String,
        code: String,
        message: String,
    },
}

impl RuntimeEvent {
    pub fn code(&self) -> &str {
        match self {
            Self::SourceFailure { code, .. } | Self::RemoteError { code, .. } => code,
            Self::SourceSlow { .. } => "SLOW_SOURCE",
            Self::RemoteResult { .. } => "OK",
        }
    }
}

/// 事件接收器。M1 用记录型 sink 测试；M2 接 Tauri emit。
pub trait EventSink: Send + Sync {
    fn emit(&self, event: RuntimeEvent);
}

/// 丢弃所有事件的 sink，用于不需要诊断的调用方。
#[derive(Debug, Default)]
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: RuntimeEvent) {}
}

/// 记录型 sink，测试与 CLI 诊断使用。
#[derive(Debug, Default)]
pub struct RecordingSink {
    events: Mutex<Vec<RuntimeEvent>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<RuntimeEvent> {
        self.events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }

    pub fn failures(&self) -> Vec<RuntimeEvent> {
        self.events()
            .into_iter()
            .filter(|event| matches!(event, RuntimeEvent::SourceFailure { .. }))
            .collect()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: RuntimeEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_sink_collects_events_in_order() {
        let sink = RecordingSink::new();
        sink.emit(RuntimeEvent::SourceSlow {
            query_id: "q1".to_string(),
            source_id: "local-1".to_string(),
            elapsed_ms: 30,
        });
        sink.emit(RuntimeEvent::SourceFailure {
            query_id: "q1".to_string(),
            source_id: "local-2".to_string(),
            source_name: "Broken".to_string(),
            code: "DICT_OPEN_FAILED".to_string(),
            message: "bad file".to_string(),
        });
        assert_eq!(sink.events().len(), 2);
        assert_eq!(sink.failures().len(), 1);
        assert_eq!(sink.failures()[0].code(), "DICT_OPEN_FAILED");
    }

    #[test]
    fn query_id_is_unique() {
        assert_ne!(QueryId::new(), QueryId::new());
    }
}
