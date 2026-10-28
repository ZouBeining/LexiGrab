use std::path::PathBuf;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

use crate::core::error::{IpcError, IpcResult};
use crate::platform::ScreenAnchor;

pub const MANAGER: &str = "manager";
pub const SETTINGS: &str = "settings";
pub const POPUP: &str = "popup";

const MANAGER_URL: &str = "manager.html?view=sources";
const SETTINGS_URL: &str = "settings.html?section=general";
const POPUP_URL: &str = "popup.html";

const POPUP_WIDTH: f64 = 420.0;
const POPUP_HEIGHT: f64 = 560.0;
const POPUP_EDIT_WIDTH: f64 = 560.0;
const POPUP_EDIT_HEIGHT: f64 = 640.0;

/// 显示窗口；不存在时按 label 创建。返回窗口引用方便后续操作。
pub fn show_window(app: &AppHandle, label: &str) -> IpcResult<WebviewWindow> {
    let window = ensure_window(app, label)?;
    match label {
        MANAGER | SETTINGS => {
            window
                .show()
                .map_err(|error| IpcError::internal(format!("显示窗口失败: {error}")))?;
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
        POPUP => show_popup(app, None)?,
        other => return Err(IpcError::window_not_found(other)),
    }
    Ok(window)
}

pub fn ensure_window(app: &AppHandle, label: &str) -> IpcResult<WebviewWindow> {
    if let Some(window) = app.get_webview_window(label) {
        return Ok(window);
    }
    create_window(app, label)
}

fn create_window(app: &AppHandle, label: &str) -> IpcResult<WebviewWindow> {
    let builder = match label {
        MANAGER => WebviewWindowBuilder::new(app, MANAGER, WebviewUrl::App(MANAGER_URL.into()))
            .title("LexiGrab")
            .inner_size(980.0, 700.0)
            .min_inner_size(900.0, 600.0)
            .center()
            .visible(false),
        SETTINGS => WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App(SETTINGS_URL.into()))
            .title("LexiGrab 设置")
            .inner_size(900.0, 700.0)
            .min_inner_size(860.0, 640.0)
            .center()
            .visible(false),
        POPUP => WebviewWindowBuilder::new(app, POPUP, WebviewUrl::App(POPUP_URL.into()))
            .title("LexiGrab")
            .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .focused(false)
            .visible(false)
            .shadow(true),
        other => return Err(IpcError::window_not_found(other)),
    };

    // LEXIGRAB_HOME 同时用于测试与便携部署：把 WebView2 数据目录也放进该根目录。
    let builder = match webview_data_dir() {
        Some(directory) => {
            if let Err(error) = std::fs::create_dir_all(&directory) {
                tracing::warn!(error = %error, "创建 WebView 数据目录失败");
            }
            builder.data_directory(directory)
        }
        None => builder,
    };

    let window = builder
        .build()
        .map_err(|error| IpcError::internal(format!("创建窗口 {label} 失败: {error}")))?;

    install_hide_on_close(&window);
    Ok(window)
}

fn webview_data_dir() -> Option<PathBuf> {
    std::env::var_os("LEXIGRAB_HOME").map(|root| PathBuf::from(root).join("webview2"))
}

/// 记录主线程 id（Windows 弹窗操作需要判断是否已在主线程）。#[cfg(windows)]
pub fn mark_main_thread() {
    crate::platform::windows::popup::mark_main_thread();
}

/// 非 Windows 平台无需主线程调度记录。
#[cfg(not(windows))]
pub fn mark_main_thread() {}

/// manager / settings / popup 关闭时只隐藏，退出统一走托盘。
pub fn install_hide_on_close(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            if let Err(error) = handle.hide() {
                tracing::warn!(error = %error, "隐藏窗口失败");
            }
        }
    });
}

/// 显示弹窗。快速模式按锚点定位且不抢焦点；编辑模式激活窗口。
pub fn show_popup(app: &AppHandle, anchor: Option<ScreenAnchor>) -> IpcResult<()> {
    let window = ensure_window(app, POPUP)?;
    let interactive = app
        .state::<crate::core::state::AppState>()
        .popup_interactive();

    if interactive {
        let _ = window.set_size(tauri::LogicalSize::new(POPUP_EDIT_WIDTH, POPUP_EDIT_HEIGHT));
        window
            .show()
            .map_err(|error| IpcError::internal(format!("显示弹窗失败: {error}")))?;
        let _ = window.set_focus();
        popup_activate(app, &window)?;
        return Ok(());
    }

    let _ = window.set_size(tauri::LogicalSize::new(POPUP_WIDTH, POPUP_HEIGHT));
    popup_show_no_activate(app, &window, anchor)
}

pub fn hide_popup(app: &AppHandle) -> IpcResult<()> {
    let state = app.state::<crate::core::state::AppState>();
    if let Some(window) = app.get_webview_window(POPUP) {
        let was_interactive = state.popup_interactive();
        popup_hide(app, &window)?;
        tracing::debug!(was_interactive, "已隐藏弹窗");
        if was_interactive {
            state.set_popup_interactive(false);
            popup_restore_foreground(app);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn popup_show_no_activate(
    app: &AppHandle,
    window: &WebviewWindow,
    anchor: Option<ScreenAnchor>,
) -> IpcResult<()> {
    crate::platform::windows::popup::show_no_activate(app, window, anchor)
}

#[cfg(not(windows))]
fn popup_show_no_activate(
    _app: &AppHandle,
    window: &WebviewWindow,
    anchor: Option<ScreenAnchor>,
) -> IpcResult<()> {
    window
        .show()
        .map_err(|error| IpcError::internal(format!("显示弹窗失败: {error}")))?;
    if let Some(anchor) = anchor {
        let _ = window.set_position(tauri::LogicalPosition::new(anchor.x, anchor.y));
    }
    Ok(())
}

#[cfg(windows)]
fn popup_activate(app: &AppHandle, window: &WebviewWindow) -> IpcResult<()> {
    crate::platform::windows::popup::activate(app, window)
}

#[cfg(not(windows))]
fn popup_activate(_app: &AppHandle, _window: &WebviewWindow) -> IpcResult<()> {
    Ok(())
}

#[cfg(windows)]
fn popup_hide(app: &AppHandle, window: &WebviewWindow) -> IpcResult<()> {
    crate::platform::windows::popup::hide(app, window)
}

#[cfg(not(windows))]
fn popup_hide(_app: &AppHandle, window: &WebviewWindow) -> IpcResult<()> {
    window
        .hide()
        .map_err(|error| IpcError::internal(format!("隐藏弹窗失败: {error}")))
}

#[cfg(windows)]
fn popup_restore_foreground(app: &AppHandle) {
    crate::platform::windows::popup::restore_foreground(app);
}

#[cfg(not(windows))]
fn popup_restore_foreground(_app: &AppHandle) {}
