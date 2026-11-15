//! 后端库：M0 窗口/选区捕获 + M1 核心链路 + M2 桌面 MVP。
//! 业务逻辑放在 core/sources/parsers/anki/db，Tauri 命令只做薄封装。

pub mod anki;
pub mod bridge;
pub mod commands;
pub mod core;
pub mod db;
pub mod parsers;
pub mod platform;
pub mod sources;

use tauri::http::{header, Method, Request, Response, StatusCode};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::core::error::IpcError;
use crate::core::state::AppState;

/// 本地资源协议名；Windows 上渲染为 `http://mdx-res.localhost/...`。
pub const RESOURCE_SCHEME: &str = "mdx-res";

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

    let state = match AppState::new(paths) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("初始化应用状态失败: {error}");
            return;
        }
    };
    if let Some(warning) = state.config_warning() {
        tracing::warn!(%warning, "配置文件已回退默认值");
    }
    let start_minimized = state.settings().general.start_minimized;
    let hotkey = state.settings().general.hotkey.clone();

    let builder = tauri::Builder::default()
        // 单实例插件必须最先注册，第二个进程只通知已有实例后退出。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_or_log(app, core::window::MANAGER);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .register_uri_scheme_protocol(RESOURCE_SCHEME, |ctx, request| {
            handle_resource_request(ctx.app_handle(), request)
        })
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
            commands::lookup,
            commands::get_query_meta,
            commands::list_sources,
            commands::add_source,
            commands::update_source,
            commands::remove_source,
            commands::get_source_statuses,
            commands::test_source_query,
            commands::export_to_anki,
            commands::check_card_context,
            commands::get_anki_decks,
            commands::get_anki_models,
            commands::probe_anki,
            commands::get_settings,
            commands::update_settings,
            commands::open_config_dir,
            commands::get_bridge_status,
            commands::regenerate_bridge_token,
        ])
        .setup(move |app| {
            core::window::mark_main_thread();
            let handle = app.handle().clone();
            if let Err(error) = setup_tray(&handle) {
                tracing::warn!(error = %error, "托盘初始化失败");
            }
            // 窗口全部由工厂创建，便于统一注入 WebView2 数据目录（测试/便携部署）。
            // 三个窗口在启动时预创建：popup / settings 保持隐藏，避免在命令线程里创建 WebView 造成阻塞。
            if start_minimized {
                tracing::info!("start_minimized = true，跳过显示管理器");
            } else if let Err(error) = core::window::show_window(&handle, core::window::MANAGER) {
                tracing::error!(code = %error.code, detail = ?error.detail, "创建管理器窗口失败");
            }
            for label in [core::window::POPUP, core::window::SETTINGS] {
                if let Err(error) = core::window::ensure_window(&handle, label) {
                    tracing::warn!(code = %error.code, label, "预创建窗口失败");
                }
            }

            let state = app.state::<AppState>();
            match core::shortcut::register_hotkey(&handle, &hotkey) {
                Ok(()) => state.set_hotkey_status(&hotkey, true, None),
                Err(error) => {
                    tracing::warn!(code = %error.code, "全局热键注册失败");
                    state.set_hotkey_status(&hotkey, false, Some(error.message.clone()));
                    if let Err(emit_error) = handle.emit(
                        "runtime://hotkey-error",
                        serde_json::json!({ "message": error.message }),
                    ) {
                        tracing::warn!(error = %emit_error, "发送热键错误事件失败");
                    }
                }
            }

            // 大词典打开与 Anki 探测都放到后台，避免拖慢首屏。
            spawn_registry_load(&handle);
            spawn_anki_probe(&handle);

            // 桥接只监听 127.0.0.1；端口占用时状态为 failed，不自动换端口。
            let bridge_settings = app.state::<AppState>().settings().bridge;
            app.manage(bridge::BridgeService::new(handle.clone()));
            app.state::<bridge::BridgeService>().apply(&bridge_settings);
            Ok(())
        });

    match builder.build(tauri::generate_context!()) {
        Ok(app) => app.run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                tracing::info!("应用退出，注销热键");
                let _ =
                    tauri_plugin_global_shortcut::GlobalShortcutExt::global_shortcut(app_handle)
                        .unregister_all();
                app_handle.state::<bridge::BridgeService>().stop();
            }
        }),
        Err(error) => tracing::error!(error = %error, "应用启动失败"),
    }
}

/// 后台打开配置中的词典源，完成后广播每源状态。
fn spawn_registry_load(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let settings = state.settings();
        state.rebuild_registry(&settings);
        for entry in state.source_states() {
            let payload = match &entry.status {
                sources::SourceStatus::Loading => serde_json::json!({
                    "source_id": entry.id,
                    "path": "",
                    "phase": "loading",
                }),
                sources::SourceStatus::Ready { entry_count } => serde_json::json!({
                    "source_id": entry.id,
                    "path": "",
                    "phase": "ready",
                    "entry_count": entry_count,
                }),
                sources::SourceStatus::Failed { message } => serde_json::json!({
                    "source_id": entry.id,
                    "path": "",
                    "phase": "failed",
                    "message": message,
                }),
            };
            if let Err(error) = handle.emit("dict://index-progress", payload) {
                tracing::warn!(error = %error, "发送词典索引进度失败");
            }
        }
    });
}

/// 启动时异步探测 AnkiConnect，结果通过事件通知设置页。
fn spawn_anki_probe(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let settings = handle.state::<AppState>().settings();
        let status = match crate::anki::client::AnkiClient::new(&settings.anki.endpoint)
            .and_then(|client| client.version())
        {
            Ok(version) => serde_json::json!({ "available": true, "version": version }),
            Err(error) => serde_json::json!({
                "available": false,
                "version": null,
                "message": error.to_string(),
            }),
        };
        if let Err(error) = handle.emit("runtime://anki-status", status) {
            tracing::warn!(error = %error, "发送 Anki 状态事件失败");
        }
    });
}

/// `mdx-res` 请求处理：只允许 GET/HEAD、白名单 MIME、白名单路径。
fn handle_resource_request(app: &AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return text_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "only GET and HEAD are allowed",
        );
    }
    let raw = request.uri().to_string();
    let parsed = match core::resources::parse_request(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            let status =
                StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
            return text_response(status, &error.to_string());
        }
    };
    let mime = match core::resources::mime_for_path(&parsed.path) {
        Some(mime) => mime,
        None => return text_response(StatusCode::NOT_FOUND, "resource type not allowed"),
    };

    let state = app.state::<AppState>();
    let resource = match state.resolve_resource(&parsed.dict_id, &parsed.path) {
        Ok(resource) => resource,
        Err(error) => {
            let status = StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::NOT_FOUND);
            return text_response(status, &error.to_string());
        }
    };
    let body = if request.method() == Method::HEAD {
        Vec::new()
    } else {
        resource.bytes
    };
    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        mime.parse()
            .unwrap_or_else(|_| header::HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("public, max-age=3600"),
    );
    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    response
}

fn text_response(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    let mut response = Response::new(message.as_bytes().to_vec());
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let manager = MenuItem::with_id(app, "manager", "打开管理器", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&manager, &settings, &quit])?;

    let mut builder = TrayIconBuilder::with_id("lexigrab")
        .tooltip("LexiGrab")
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
