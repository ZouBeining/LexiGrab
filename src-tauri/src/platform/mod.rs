// SPDX-License-Identifier: AGPL-3.0-only
pub mod selection;

#[cfg(windows)]
pub mod windows;

pub use selection::{CaptureMethod, CaptureResult, ScreenAnchor, SelectionContext};

/// 捕获当前前台应用的选区。失败时返回结构化错误而不是 panic。
pub fn capture_selection() -> CaptureResult {
    #[cfg(windows)]
    {
        windows::capture::capture_selection()
    }
    #[cfg(not(windows))]
    {
        CaptureResult {
            selection: None,
            error: Some(CaptureError {
                code: "CAPTURE_UNAVAILABLE".to_string(),
                message: "当前平台不支持选区捕获".to_string(),
                detail: Some(std::env::consts::OS.to_string()),
            }),
        }
    }
}
