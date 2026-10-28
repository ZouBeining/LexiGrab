use tauri::{AppHandle, Manager, State};

use crate::core::error::{IpcError, IpcResult};
use crate::core::state::{AppState, CaptureStats, RuntimeStatus};
use crate::core::{shortcut, window};
use crate::platform::selection::CaptureError;
use crate::platform::{CaptureResult, SelectionContext};

/// 显示窗口（manager / settings / popup）。
#[tauri::command]
pub fn show_window(app: AppHandle, label: String) -> IpcResult<()> {
    window::show_window(&app, &label).map(|_| ())
}

/// 打开白名单窗口，供弹窗跳转使用。
#[tauri::command]
pub fn open_window(app: AppHandle, target: String) -> IpcResult<()> {
    match target.as_str() {
        window::MANAGER | window::SETTINGS => {
            window::show_window(&app, target.as_str()).map(|_| ())
        }
        _ => Err(IpcError::window_not_found(&target)),
    }
}

#[tauri::command]
pub fn hide_popup(app: AppHandle) -> IpcResult<()> {
    tracing::debug!("前端请求隐藏弹窗");
    window::hide_popup(&app)
}

#[tauri::command]
pub fn set_popup_interactive(app: AppHandle, interactive: bool) -> IpcResult<()> {
    tracing::debug!(interactive, "前端请求切换弹窗编辑模式");
    let state = app.state::<AppState>();
    state.set_popup_interactive(interactive);
    if interactive {
        window::show_popup(&app, None)
    } else {
        window::hide_popup(&app)
    }
}

#[tauri::command]
pub fn get_current_selection(state: State<'_, AppState>) -> Option<SelectionContext> {
    state.last_selection()
}

/// M0 调试命令：绕过热键直接捕获一次，便于在管理器里查看结果。
/// 必须离开主线程执行，否则目标窗口（可能是自身）无法及时处理合成按键。
#[tauri::command]
pub async fn debug_trigger_capture(app: AppHandle) -> CaptureResult {
    let result =
        match tauri::async_runtime::spawn_blocking(crate::platform::capture_selection).await {
            Ok(result) => result,
            Err(error) => CaptureResult::failure(CaptureError {
                code: "INTERNAL".to_string(),
                message: "捕获线程异常".to_string(),
                detail: Some(error.to_string()),
            }),
        };
    app.state::<AppState>().record_capture(&result);
    shortcut::log_capture_result(&result);
    result
}

#[tauri::command]
pub fn get_capture_stats(state: State<'_, AppState>) -> CaptureStats {
    state.capture_stats()
}

/// M0 专用：弹窗挂载时读取最近一次结果，用于恢复捕获失败态。
#[tauri::command]
pub fn get_last_capture(state: State<'_, AppState>) -> Option<CaptureResult> {
    state.last_capture()
}

#[tauri::command]
pub fn get_runtime_status(app: AppHandle, state: State<'_, AppState>) -> RuntimeStatus {
    let popup_visible = app
        .get_webview_window(window::POPUP)
        .and_then(|popup| popup.is_visible().ok())
        .unwrap_or(false);
    RuntimeStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        hotkey: state.hotkey_status(),
        config_dir: state.paths.config_dir.to_string_lossy().to_string(),
        log_dir: state.paths.log_dir.to_string_lossy().to_string(),
        popup_visible,
    }
}
