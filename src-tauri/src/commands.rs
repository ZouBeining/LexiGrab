// SPDX-License-Identifier: AGPL-3.0-only
//! Tauri 命令层：只做参数整理、阻塞边界与错误映射，业务逻辑在 core / anki / db。

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::anki::client::AnkiClient;
use crate::anki::export::{export_to_anki as run_export, ExportDeps};
use crate::core::config::Settings;
use crate::core::entry::{AnkiPayload, Entry, SourceConfig, SourceKind};
use crate::core::error::{IpcError, IpcResult};
use crate::core::events::TauriEventSink;
use crate::core::registry::SourceStateEntry;
use crate::core::state::{AppState, CaptureStats, QueryMeta, RuntimeStatus};
use crate::core::{shortcut, window};
use crate::db::cache::CacheStats;
use crate::db::CardContext;
use crate::platform::selection::CaptureError;
use crate::platform::{CaptureResult, SelectionContext};
use crate::sources::local_file::LocalFileSource;
use crate::sources::DictionarySource;

/// AnkiConnect 探测结果。
#[derive(Debug, Clone, Serialize)]
pub struct AnkiStatus {
    pub available: bool,
    pub version: Option<u32>,
    pub message: Option<String>,
}

/// 模板与字段，供设置页字段映射下拉框。
#[derive(Debug, Clone, Serialize)]
pub struct AnkiModelInfo {
    pub name: String,
    pub fields: Vec<String>,
}

/// 查询测试结果。
#[derive(Debug, Clone, Serialize)]
pub struct TestFailure {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TestResult {
    pub entries: Vec<Entry>,
    pub source_name: String,
    pub elapsed_ms: f64,
    /// 远程响应前 4KB；本地源为 null。
    pub raw_preview: Option<String>,
    /// 远程查询失败时的可读错误；本地源失败仍以 IPC 错误返回。
    pub error: Option<TestFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportResponse {
    pub note_id: i64,
    pub exact_context_seen: bool,
}

/// 把 `spawn_blocking` 的 JoinError 映射成 IPC 错误。
fn join_error(error: impl std::fmt::Display) -> IpcError {
    IpcError::internal(format!("后台任务异常: {error}"))
}

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
        Ok(())
    }
}

#[tauri::command]
pub fn get_current_selection(state: State<'_, AppState>) -> Option<SelectionContext> {
    state.last_selection()
}

/// 调试命令：绕过热键直接捕获一次，便于在管理器里查看结果。
#[tauri::command]
pub async fn debug_trigger_capture(app: AppHandle) -> CaptureResult {
    let handle = app.clone();
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
    let _ = handle;
    result
}

#[tauri::command]
pub fn get_capture_stats(state: State<'_, AppState>) -> CaptureStats {
    state.capture_stats()
}

/// 弹窗挂载时读取最近一次结果，用于恢复捕获失败态。
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
        cache_path: state.paths.cache_db.to_string_lossy().to_string(),
        popup_visible,
        sources_ready: state.sources_ready(),
    }
}

/// 同步查询：本地与缓存结果放在 blocking 线程；远程任务转后台线程，结果通过事件追加。
#[tauri::command]
pub async fn lookup(
    app: AppHandle,
    query: String,
    source_ids: Option<Vec<String>>,
    query_id: Option<String>,
    sentence: Option<String>,
) -> IpcResult<Vec<Entry>> {
    let id = query_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let handle = app.clone();
    let lookup_id = id.clone();
    let lookup_query = query.clone();
    let (entries, jobs) = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let sink = TauriEventSink::new(handle.clone());
        let outcome =
            state.lookup_planned(&lookup_query, source_ids.as_deref(), &lookup_id, &sink)?;
        Ok::<_, IpcError>((outcome.entries, outcome.remote_jobs))
    })
    .await
    .map_err(join_error)??;

    for job in jobs {
        let handle = app.clone();
        let query = query.clone();
        let query_id = id.clone();
        std::thread::spawn(move || {
            let state = handle.state::<AppState>();
            let sink = TauriEventSink::new(handle.clone());
            state.run_remote_job(&job, &query, &query_id, &sink);
        });
    }

    if let Some(sentence) = sentence
        .as_deref()
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
    {
        spawn_sense_resolution(&app, &id, &query, sentence, &entries);
    }

    Ok(entries)
}

/// 后台解析语境推荐；首屏不等待，结果通过 `lookup://sense-result` 通知。
fn spawn_sense_resolution(
    app: &AppHandle,
    query_id: &str,
    headword: &str,
    sentence: &str,
    entries: &[Entry],
) {
    let state = app.state::<AppState>();
    if !state.settings().sense.enabled || entries.is_empty() {
        return;
    }
    state.mark_sense_pending(query_id);
    let handle = app.clone();
    let query_id = query_id.to_string();
    let headword = headword.to_string();
    let sentence = sentence.to_string();
    let entries = entries.to_vec();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let resolution =
            state.run_sense_resolution(&query_id, &headword, Some(&sentence), &entries);
        state.store_sense_resolution(&query_id, &resolution);
        if let Err(error) = handle.emit("lookup://sense-result", &resolution) {
            tracing::warn!(error = %error, "发送语境推荐事件失败");
        }
    });
}

#[tauri::command]
pub fn get_query_meta(state: State<'_, AppState>, query_id: String) -> Option<QueryMeta> {
    state.query_meta(&query_id)
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Vec<SourceConfig> {
    state.settings().sources
}

#[tauri::command]
pub fn get_source_statuses(state: State<'_, AppState>) -> Vec<SourceStateEntry> {
    state.source_states()
}

/// 新增数据源：生成 id、校验重复路径、保存后后台打开词典。
#[tauri::command]
pub fn add_source(
    app: AppHandle,
    state: State<'_, AppState>,
    mut config: SourceConfig,
) -> IpcResult<SourceConfig> {
    if config.id.trim().is_empty() {
        config.id = SourceConfig::generate_id(&config.kind);
    }
    let mut settings = state.settings();
    if settings
        .sources
        .iter()
        .any(|existing| existing.id == config.id)
    {
        return Err(IpcError::new("SOURCE_DUPLICATE", "数据源 id 已存在"));
    }
    ensure_no_duplicate_paths(&state, &settings, &config)?;
    settings.sources.push(config.clone());
    state.replace_settings(settings)?;
    spawn_reload(&app);
    Ok(config)
}

/// 更新数据源：保留 id，保存后后台重载。
#[tauri::command]
pub fn update_source(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    mut config: SourceConfig,
) -> IpcResult<SourceConfig> {
    let mut settings = state.settings();
    let index = settings
        .sources
        .iter()
        .position(|source| source.id == id)
        .ok_or_else(|| IpcError::new("SOURCE_NOT_FOUND", format!("数据源不存在: {id}")))?;
    config.id = id;
    ensure_no_duplicate_paths(&state, &settings, &config)?;
    settings.sources[index] = config.clone();
    state.replace_settings(settings)?;
    // 源配置变化后旧缓存不再可信。
    if let Err(error) = state.cache().invalidate_source(&config.id) {
        tracing::warn!(error = %error, source_id = %config.id, "清理数据源缓存失败");
    }
    spawn_reload(&app);
    Ok(config)
}

/// 删除数据源：只删配置，不删除磁盘文件。
#[tauri::command]
pub fn remove_source(app: AppHandle, state: State<'_, AppState>, id: String) -> IpcResult<()> {
    let mut settings = state.settings();
    let before = settings.sources.len();
    settings.sources.retain(|source| source.id != id);
    if settings.sources.len() == before {
        return Err(IpcError::new(
            "SOURCE_NOT_FOUND",
            format!("数据源不存在: {id}"),
        ));
    }
    state.replace_settings(settings)?;
    if let Err(error) = state.cache().invalidate_source(&id) {
        tracing::warn!(error = %error, source_id = %id, "清理数据源缓存失败");
    }
    spawn_reload(&app);
    Ok(())
}

/// 用草稿配置查询单个源，不写配置、不写缓存、不触发弹窗。
#[tauri::command]
pub async fn test_source_query(
    app: AppHandle,
    config: SourceConfig,
    query: String,
) -> IpcResult<TestResult> {
    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app_handle.state::<AppState>();
        let started = Instant::now();
        match &config.kind {
            SourceKind::LocalFile { .. } => {
                let source = LocalFileSource::open(config.clone(), &state.paths.config_dir);
                let entries = source.lookup(&query).map_err(IpcError::from)?;
                Ok(TestResult {
                    entries,
                    source_name: config.name,
                    elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                    raw_preview: None,
                    error: None,
                })
            }
            SourceKind::RemoteHttp { .. } => {
                let outcome = crate::sources::remote_http::test_query(
                    &config,
                    &state.paths.config_dir,
                    &query,
                );
                let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
                match outcome {
                    Ok(outcome) => Ok(TestResult {
                        entries: outcome.entries,
                        source_name: config.name,
                        elapsed_ms,
                        raw_preview: outcome.raw_preview,
                        error: None,
                    }),
                    Err(failure) => Ok(TestResult {
                        entries: Vec::new(),
                        source_name: config.name,
                        elapsed_ms,
                        raw_preview: failure.raw_preview,
                        error: Some(TestFailure {
                            code: failure.error.code().to_string(),
                            message: failure.error.to_string(),
                        }),
                    }),
                }
            }
        }
    })
    .await
    .map_err(join_error)?
}

/// 导出到 Anki：始终允许重复；同语境提示只作为返回值交给 UI。
#[tauri::command]
pub async fn export_to_anki(app: AppHandle, payload: AnkiPayload) -> IpcResult<ExportResponse> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let settings = state.settings();
        let cards = state.cards();
        export_to_anki_command(&settings, cards.as_ref(), &payload)
    })
    .await
    .map_err(join_error)?
}

fn export_to_anki_command(
    settings: &Settings,
    cards: &crate::db::CardStore,
    payload: &AnkiPayload,
) -> IpcResult<ExportResponse> {
    let deps = ExportDeps {
        settings: &settings.anki,
        cards,
    };
    run_export(deps, payload)
        .map(|outcome| ExportResponse {
            note_id: outcome.note_id,
            exact_context_seen: outcome.exact_context_seen,
        })
        .map_err(IpcError::from)
}

/// 保存前检查同词同句同释义是否已经存过。
///
/// 指纹必须与导出端一致：先剥离本地媒体再做纯文本归一化，否则含图片/音频的
/// 释义在第二次保存时不会命中已存语境。
#[tauri::command]
pub async fn check_card_context(app: AppHandle, payload: AnkiPayload) -> IpcResult<CardContext> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let cards = state.cards();
        let definition_text =
            crate::anki::export::context_definition_text(&payload.definition_html);
        cards
            .check_context(&payload.word, payload.sentence.as_deref(), &definition_text)
            .map_err(IpcError::from)
    })
    .await
    .map_err(join_error)?
}

fn anki_client(settings: &Settings) -> IpcResult<AnkiClient> {
    AnkiClient::new(&settings.anki.endpoint).map_err(IpcError::from)
}

#[tauri::command]
pub async fn get_anki_decks(app: AppHandle) -> IpcResult<Vec<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app.state::<AppState>().settings();
        anki_client(&settings)?.deck_names().map_err(IpcError::from)
    })
    .await
    .map_err(join_error)?
}

#[tauri::command]
pub async fn get_anki_models(app: AppHandle) -> IpcResult<Vec<AnkiModelInfo>> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app.state::<AppState>().settings();
        let client = anki_client(&settings)?;
        let names = client.model_names().map_err(IpcError::from)?;
        let mut models = Vec::with_capacity(names.len());
        for name in names {
            let fields = client.model_field_names(&name).map_err(IpcError::from)?;
            models.push(AnkiModelInfo { name, fields });
        }
        Ok(models)
    })
    .await
    .map_err(join_error)?
}

/// 只读探测 AnkiConnect，不写测试卡片。
#[tauri::command]
pub async fn probe_anki(app: AppHandle) -> AnkiStatus {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app.state::<AppState>().settings();
        match AnkiClient::new(&settings.anki.endpoint).and_then(|client| client.version()) {
            Ok(version) => AnkiStatus {
                available: true,
                version: Some(version),
                message: None,
            },
            Err(error) => AnkiStatus {
                available: false,
                version: None,
                message: Some(error.to_string()),
            },
        }
    })
    .await
    .unwrap_or_else(|error| AnkiStatus {
        available: false,
        version: None,
        message: Some(format!("Anki 探测线程异常: {error}")),
    })
}

/// 清理结果的统一形状：`{ removed }`。
#[derive(Debug, Clone, Serialize)]
pub struct ClearResult {
    pub removed: u64,
}

/// 缓存统计：设置页“缓存”分区。
#[tauri::command]
pub async fn get_cache_stats(app: AppHandle) -> IpcResult<CacheStats> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state.cache().stats().map_err(IpcError::from)
    })
    .await
    .map_err(join_error)?
}

/// 清除词典缓存；保留 `saved_cards` 历史与语境缓存（契约 §3）。
#[tauri::command]
pub async fn clear_lookup_cache(app: AppHandle) -> IpcResult<ClearResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state
            .cache()
            .clear_lookup()
            .map(|removed| ClearResult { removed })
            .map_err(IpcError::from)
    })
    .await
    .map_err(join_error)?
}

/// 读取已完成或进行中的语境推荐；未知/过期返回 null。
#[tauri::command]
pub fn get_sense_resolution(
    state: State<'_, AppState>,
    query_id: String,
) -> Option<crate::core::sense::SenseResolution> {
    state.sense_resolution(&query_id)
}

/// 设置页测试选择器：只测连通与返回结构，不写缓存、不写卡。
#[tauri::command]
pub async fn test_sense_selector(
    app: AppHandle,
    word: String,
    sentence: String,
) -> IpcResult<crate::core::sense::SenseResolution> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let settings = state.settings();
        let word = word.trim().to_string();
        let sentence = sentence.trim().to_string();
        let mut resolution = crate::core::sense::SenseResolution {
            query_id: "test".to_string(),
            headword: word.clone(),
            status: crate::core::sense::SenseStatus::Disabled,
            selected_sense_id: None,
            confidence: None,
            selector_model: None,
            ai: None,
            error: None,
            candidates: Vec::new(),
        };
        if !settings.sense.enabled {
            resolution.error = Some("SENSE_DISABLED: 语境辅助未启用".to_string());
            return Ok(resolution);
        }
        if word.is_empty() || sentence.is_empty() {
            resolution.status = crate::core::sense::SenseStatus::NoContext;
            resolution.error = Some("SENSE_NO_CONTEXT: 需要单词与原句".to_string());
            return Ok(resolution);
        }
        match crate::core::sense::select(&settings.sense, &word, &sentence, &[]) {
            Ok(reply) => {
                let confidence = reply.confidence.unwrap_or(0.0);
                resolution.status = if confidence >= settings.sense.high_confidence {
                    crate::core::sense::SenseStatus::Selected
                } else if confidence > settings.sense.low_confidence {
                    crate::core::sense::SenseStatus::Ambiguous
                } else {
                    crate::core::sense::SenseStatus::LowConfidence
                };
                resolution.confidence = Some(confidence);
                resolution.selector_model = reply.model;
            }
            Err(error) => {
                resolution.status = crate::core::sense::SenseStatus::Unavailable;
                resolution.error = Some(format!("{}: {}", error.code(), error.message()));
            }
        }
        Ok(resolution)
    })
    .await
    .map_err(join_error)?
}

/// 设置页测试 LLM：不写缓存、不写卡；未启用时仍允许测试连通性。
#[tauri::command]
pub async fn test_llm(
    app: AppHandle,
    word: String,
    sentence: String,
    senses: Option<Vec<crate::core::sense::SenseCandidate>>,
) -> IpcResult<crate::core::sense::AiDefinition> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut settings = state.settings().llm;
        settings.enabled = true;
        let senses = senses.unwrap_or_default();
        crate::core::sense::generate(&settings, word.trim(), sentence.trim(), &senses, None)
            .map_err(|error| IpcError::new(error.code(), error.message()))
    })
    .await
    .map_err(join_error)?
}

/// 清除语境缓存。
#[tauri::command]
pub async fn clear_sense_cache(app: AppHandle) -> IpcResult<ClearResult> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        state
            .cache()
            .clear_sense()
            .map(|removed| ClearResult { removed })
            .map_err(IpcError::from)
    })
    .await
    .map_err(join_error)?
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

/// 桥接运行状态：设置页与扩展配对的信息来源。
#[tauri::command]
pub fn get_bridge_status(
    bridge: State<'_, crate::bridge::BridgeService>,
) -> crate::bridge::BridgeStatus {
    bridge.status()
}

/// 轮换桥接令牌：写盘后旧令牌立即 401，扩展需重新配对。
#[tauri::command]
pub fn regenerate_bridge_token(app: AppHandle, state: State<'_, AppState>) -> IpcResult<String> {
    let mut settings = state.settings();
    settings.bridge.token = crate::core::config::generate_bridge_token();
    let saved = state.replace_settings(settings)?;
    app.state::<crate::bridge::BridgeService>()
        .apply(&saved.bridge);
    Ok(saved.bridge.token)
}

/// 保存设置：先校验，热键变化时先尝试注册；保存失败恢复旧热键。
#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> IpcResult<Settings> {
    let previous = state.settings();
    settings.validate()?;
    let hotkey_changed = previous.general.hotkey != settings.general.hotkey;
    if hotkey_changed {
        on_main_thread(&app, &previous.general.hotkey, &settings.general.hotkey)?;
    }
    match state.replace_settings(settings.clone()) {
        Ok(saved) => {
            if hotkey_changed {
                state.set_hotkey_status(&saved.general.hotkey, true, None);
            }
            spawn_reload(&app);
            // 端口/启用变化会重启桥接；端口占用只进状态，不回滚已保存的配置。
            app.state::<crate::bridge::BridgeService>()
                .apply(&saved.bridge);
            Ok(saved)
        }
        Err(error) => {
            if hotkey_changed {
                let _ = on_main_thread(&app, &settings.general.hotkey, &previous.general.hotkey);
            }
            Err(error)
        }
    }
}

/// 全局热键由 tao 事件循环管理，切换时回到主线程执行，避免平台差异。
fn on_main_thread(app: &AppHandle, old: &str, new: &str) -> IpcResult<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let handle = app.clone();
    let old = old.to_string();
    let new = new.to_string();
    app.run_on_main_thread(move || {
        let _ = sender.send(shortcut::replace_hotkey(&handle, &old, &new));
    })
    .map_err(|error| IpcError::internal(format!("切换热键调度失败: {error}")))?;
    receiver
        .recv()
        .map_err(|error| IpcError::internal(format!("切换热键未返回: {error}")))?
}

/// 用系统文件管理器打开目录；失败映射为可读的 IPC 错误。
fn open_dir(app: &AppHandle, path: &Path, label: &str) -> IpcResult<()> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|error| IpcError::internal(format!("打开{label}失败: {error}")))
}

#[tauri::command]
pub fn open_config_dir(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    open_dir(&app, &state.paths.config_dir, "配置目录")
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    open_dir(&app, &state.paths.log_dir, "日志目录")
}

/// 缓存是 SQLite 文件（`cache.db`）；打开其父目录，便于用户定位缓存库。
#[tauri::command]
pub fn open_cache_dir(app: AppHandle, state: State<'_, AppState>) -> IpcResult<()> {
    let cache_dir = state
        .paths
        .cache_db
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| state.paths.cache_db.clone());
    open_dir(&app, &cache_dir, "缓存目录")
}

/// 后台重建注册表并广播状态，避免大词典打开阻塞 UI 线程。
fn spawn_reload(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        let state = handle.state::<AppState>();
        let settings = state.settings();
        state.rebuild_registry(&settings);
        emit_index_progress(&handle);
    });
}

fn emit_index_progress(app: &AppHandle) {
    let state = app.state::<AppState>();
    for entry in state.source_states() {
        let payload = match &entry.status {
            crate::sources::SourceStatus::Loading => serde_json::json!({
                "source_id": entry.id,
                "path": "",
                "phase": "loading",
            }),
            crate::sources::SourceStatus::Ready { entry_count } => serde_json::json!({
                "source_id": entry.id,
                "path": "",
                "phase": "ready",
                "entry_count": entry_count,
            }),
            crate::sources::SourceStatus::Failed { message } => serde_json::json!({
                "source_id": entry.id,
                "path": "",
                "phase": "failed",
                "message": message,
            }),
        };
        if let Err(error) = app.emit("dict://index-progress", payload) {
            tracing::warn!(error = %error, "发送词典索引进度失败");
        }
    }
}

/// 同一路径不得出现在多个源中；错误码使用 SOURCE_DUPLICATE 便于 UI 指认冲突源。
fn ensure_no_duplicate_paths(
    state: &AppState,
    settings: &Settings,
    candidate: &SourceConfig,
) -> IpcResult<()> {
    let candidate_paths = resolve_paths(&state.paths.config_dir, candidate);
    for existing in &settings.sources {
        if existing.id == candidate.id {
            continue;
        }
        let existing_paths = resolve_paths(&state.paths.config_dir, existing);
        for path in &candidate_paths {
            if existing_paths.iter().any(|other| other == path) {
                return Err(IpcError::new(
                    "SOURCE_DUPLICATE",
                    format!("路径已属于数据源「{}」: {}", existing.name, path.display()),
                ));
            }
        }
    }
    Ok(())
}

fn resolve_paths(base_dir: &Path, config: &SourceConfig) -> Vec<PathBuf> {
    match &config.kind {
        SourceKind::LocalFile { paths } => paths
            .iter()
            .map(|path| {
                if path.is_absolute() {
                    path.clone()
                } else {
                    base_dir.join(path)
                }
            })
            .collect(),
        SourceKind::RemoteHttp { .. } => Vec::new(),
    }
}
