//! 远程 HTTP 数据源占位。M1 不实现远程 HTTP、JSONPath 与脚本沙箱。

/// 远程源在 M1 的占位类型。M2 之后再实现 `DictionarySource`。
#[derive(Debug, Default)]
pub struct RemoteHttpSource;
