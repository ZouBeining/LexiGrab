use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tracing::{debug, info, warn};

use crate::core::error::{IpcError, IpcResult};
use crate::core::state::AppState;
use crate::core::window;

pub const DEFAULT_ACCELERATOR: &str = "CmdOrCtrl+Shift+D";
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(250);

/// 注册全局热键。M2 修改热键时复用同一入口。
pub fn register_hotkey(app: &AppHandle, accelerator: &str) -> IpcResult<()> {
    let shortcut: Shortcut = accelerator.parse().map_err(|error| {
        IpcError::new("HOTKEY_CONFLICT", "快捷键格式无效").with_detail(format!("{error}"))
    })?;

    if let Err(error) = app.global_shortcut().unregister_all() {
        debug!(error = %error, "注销旧热键失败");
    }

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_capture(&handle);
            }
        })
        .map_err(|error| {
            IpcError::new("HOTKEY_CONFLICT", "快捷键被其他程序占用，请换一个组合")
                .with_detail(format!("{error}"))
        })?;

    info!(accelerator, "全局热键已注册");
    Ok(())
}

/// 只记录长度、方法、应用名与耗时，不写选中文本。
pub fn log_capture_result(result: &crate::platform::CaptureResult) {
    match (&result.selection, &result.error) {
        (Some(selection), _) => info!(
            len = selection.text.chars().count(),
            method = ?selection.method,
            app = %selection.app_name,
            elapsed_ms = selection.elapsed_ms,
            "选区捕获完成"
        ),
        (None, Some(error)) => warn!(code = %error.code, "选区捕获失败"),
        (None, None) => warn!("选区捕获返回空结果"),
    }
}

/// 热键与调试命令共用的捕获入口：去抖、捕获、记录、通知弹窗。
pub fn trigger_capture(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !state.allow_trigger(DEBOUNCE_WINDOW) {
        debug!("忽略 250ms 内的重复热键触发");
        return;
    }

    if let Some(popup) = app.get_webview_window(window::POPUP) {
        let visible = popup.is_visible().unwrap_or(false);
        debug!(visible, "热键触发前弹窗状态");
        if visible && !state.popup_interactive() {
            debug!("弹窗已显示，热键切换为隐藏");
            if let Err(error) = window::hide_popup(app) {
                warn!(code = %error.code, "隐藏弹窗失败");
            }
            return;
        }
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        let result = crate::platform::capture_selection();
        {
            let state = handle.state::<AppState>();
            state.record_capture(&result);
        }

        match &result.selection {
            Some(selection) => {
                log_capture_result(&result);
                if let Err(error) = handle.emit("popup://selection", selection) {
                    warn!(error = %error, "发送 popup://selection 失败");
                }
            }
            None => {
                log_capture_result(&result);
                if let Some(error) = &result.error {
                    if let Err(emit_error) = handle.emit("popup://capture-error", error) {
                        warn!(error = %emit_error, "发送 popup://capture-error 失败");
                    }
                }
            }
        }

        let anchor = result
            .selection
            .as_ref()
            .and_then(|selection| selection.anchor);
        if let Err(error) = window::show_popup(&handle, anchor) {
            warn!(code = %error.code, "显示弹窗失败");
        }
    });
}
