use serde::{Deserialize, Serialize};

/// 选区捕获方式，与 src/lib/types.ts 的 CaptureMethod 对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    Clipboard,
    Uia,
    Manual,
}

/// 屏幕锚点使用逻辑像素与 DPI 比例，前端按此定位弹窗。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenAnchor {
    pub x: f64,
    pub y: f64,
    pub dpi_scale: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionContext {
    pub text: String,
    /// Sentence around the selection; desktop UIA capture or the extension may fill it.
    #[serde(default)]
    pub sentence: Option<String>,
    /// Browser URL from the extension bridge; desktop capture leaves it empty.
    #[serde(default)]
    pub url: Option<String>,
    /// Page title from the extension bridge; desktop capture leaves it empty.
    #[serde(default)]
    pub title: Option<String>,
    /// Page host from the extension bridge; desktop capture leaves it empty.
    #[serde(default)]
    pub domain: Option<String>,
    pub anchor: Option<ScreenAnchor>,
    pub foreground_hwnd: i64,
    pub app_name: String,
    pub method: CaptureMethod,
    pub captured_at_ms: i64,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureResult {
    pub selection: Option<SelectionContext>,
    pub error: Option<CaptureError>,
}

impl CaptureResult {
    pub fn success(selection: SelectionContext) -> Self {
        Self {
            selection: Some(selection),
            error: None,
        }
    }

    pub fn failure(error: CaptureError) -> Self {
        Self {
            selection: None,
            error: Some(error),
        }
    }
}

/// 选中文本字符上限。超过时截断，日志只记录长度。
pub const MAX_CAPTURED_CHARS: usize = 200;

fn is_trimmed_quote(character: char) -> bool {
    matches!(
        character,
        '"' | '\'' | '“' | '”' | '‘' | '’' | '「' | '」' | '『' | '』'
    )
}

/// 清理捕获文本：去首尾空白与引号、压缩空白、保留词内连字符与撇号、截断到上限。
pub fn normalize_captured_text(raw: &str) -> String {
    let trimmed = raw
        .trim_matches(|character: char| character.is_whitespace() || is_trimmed_quote(character));

    let mut collapsed = String::with_capacity(trimmed.len());
    let mut pending_space = false;
    for character in trimmed.chars() {
        if character.is_whitespace() {
            pending_space = !collapsed.is_empty();
            continue;
        }
        if pending_space {
            collapsed.push(' ');
            pending_space = false;
        }
        collapsed.push(character);
    }

    if collapsed.chars().count() > MAX_CAPTURED_CHARS {
        let truncated: String = collapsed.chars().take(MAX_CAPTURED_CHARS).collect();
        truncated.trim_end().to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_whitespace_and_quotes() {
        assert_eq!(
            normalize_captured_text("  \u{201c}hello\u{201d}  "),
            "hello"
        );
        assert_eq!(normalize_captured_text("'serendipity'"), "serendipity");
    }

    #[test]
    fn collapses_inner_whitespace() {
        assert_eq!(
            normalize_captured_text("hello\t\tworld\n\nagain"),
            "hello world again"
        );
    }

    #[test]
    fn keeps_intra_word_hyphen_and_apostrophe() {
        assert_eq!(
            normalize_captured_text("well-known don't"),
            "well-known don't"
        );
    }

    #[test]
    fn truncates_to_max_characters() {
        let long = "a".repeat(MAX_CAPTURED_CHARS + 50);
        assert_eq!(
            normalize_captured_text(&long).chars().count(),
            MAX_CAPTURED_CHARS
        );
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(normalize_captured_text("   \n\t"), "");
    }

    #[test]
    fn keeps_non_ascii_text() {
        assert_eq!(normalize_captured_text(" 你好，世界 "), "你好，世界");
    }
}
