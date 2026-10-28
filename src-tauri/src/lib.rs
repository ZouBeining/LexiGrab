mod commands;
mod core;
mod platform;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::core::error::IpcError;
use crate::core::state::AppState;

pub fn run() {
    let paths = core::config::Paths::resolve();
    if let Err(error) = paths.ensure() {
        eprintln!("创建应用目录失败: {error}（路径: {:?}）", paths.config_dir);
    }
    let _log_guard = match core::logging::init(&paths.log_dir) {
        Ok(guard) => Some(guard),
        Err(error) => {
            eprintln!("初始化日志失败: {error}");
            None
        }
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "LexiGrab 启动");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new(paths, core::shortcut::DEFAULT_ACCELERATOR))
        .invoke_handler(tauri::generate_handler![
            commands::show_window,
            commands::open_window,
            commands::hide_popup,
            commands::set_popup_interactive,
            commands::get_current_selection,
            commands::debug_trigger_capture,
            commands::get_capture_stats,
            commands::get_last_capture,
            commands::get_runtime_status,
        ])
        .setup(|app| {
            core::window::mark_main_thread();
            let handle = app.handle().clone();
            if let Err(error) = setup_tray(&handle) {
                tracing::warn!(error = %error, "托盘初始化失败");
            }
            // 窗口全部由工厂创建，便于统一注入 WebView2 数据目录（测试/便携部署）。
            // 三个窗口在启动时预创建：popup / settings 保持隐藏，避免在命令线程里创建 WebView 造成阻塞。
            if let Err(error) = core::window::show_window(&handle, core::window::MANAGER) {
                tracing::error!(code = %error.code, detail = ?error.detail, "创建管理器窗口失败");
            }
            for label in [core::window::POPUP, core::window::SETTINGS] {
                if let Err(error) = core::window::ensure_window(&handle, label) {
                    tracing::warn!(code = %error.code, label, "预创建窗口失败");
                }
            }

            let state = app.state::<AppState>();
            match core::shortcut::register_hotkey(&handle, core::shortcut::DEFAULT_ACCELERATOR) {
                Ok(()) => state.set_hotkey_status(true, None),
                Err(error) => {
                    tracing::warn!(code = %error.code, "全局热键注册失败");
                    state.set_hotkey_status(false, Some(error.message.clone()));
                    if let Err(emit_error) = handle.emit(
                        "runtime://hotkey-error",
                        serde_json::json!({ "message": error.message }),
                    ) {
                        tracing::warn!(error = %emit_error, "发送热键错误事件失败");
                    }
                }
            }
            Ok(())
        });

    if let Err(error) = builder.run(tauri::generate_context!()) {
        tracing::error!(error = %error, "应用退出");
    }
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let manager = MenuItem::with_id(app, "manager", "打开管理器", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&manager, &settings, &quit])?;

    let mut builder = TrayIconBuilder::with_id("lexigrab")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
                tracing::info!("从托盘退出");
                app.exit(0);
            }
            "settings" => show_or_log(app, core::window::SETTINGS),
            "manager" => show_or_log(app, core::window::MANAGER),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_or_log(tray.app_handle(), core::window::MANAGER);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;
    Ok(())
}

fn show_or_log(app: &AppHandle, label: &str) {
    if let Err(error) = core::window::show_window(app, label) {
        log_window_error(&error);
    }
}

fn log_window_error(error: &IpcError) {
    tracing::warn!(code = %error.code, detail = ?error.detail, "窗口操作失败");
}
