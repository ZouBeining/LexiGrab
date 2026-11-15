/**
 * Tauri IPC 薄封装。组件与窗口只从这里导入，不直接 invoke。
 *
 * 命名约定：命令名是 Rust snake_case；参数对象的 key 按 Tauri v2 默认
 * camelCase（Tauri 宏会转成 Rust 的 snake_case 形参），不要直接传 snake_case key。
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  AnkiModel,
  AnkiPayload,
  AnkiStatus,
  BridgeStatus,
  CaptureResult,
  CaptureStats,
  CardContext,
  Entry,
  ExportResult,
  IpcError,
  OpenWindowTarget,
  QueryMeta,
  RuntimeStatus,
  SelectionContext,
  Settings,
  SourceConfig,
  SourceStateEntry,
  TestResult,
  WindowLabel,
} from "./types";

/** Rust 命令返回 IpcError 时会以对象 reject，这里归一化为可读错误。 */
export function toIpcError(error: unknown): IpcError {
  if (typeof error === "object" && error !== null && "code" in error && "message" in error) {
    const candidate = error as { code: unknown; message: unknown; detail?: unknown };
    return {
      code: typeof candidate.code === "string" ? candidate.code : "INTERNAL",
      message: typeof candidate.message === "string" ? candidate.message : "发生未知错误",
      detail: typeof candidate.detail === "string" ? candidate.detail : null,
    };
  }
  return {
    code: "INTERNAL",
    message: typeof error === "string" ? error : "发生未知错误",
    detail: null,
  };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error: unknown) {
    throw toIpcError(error);
  }
}

// ---------------------------------------------------------------------------
// 查询
// ---------------------------------------------------------------------------

export interface LookupOptions {
  query: string;
  /** 限定查询的源；省略时使用全部启用源。 */
  sourceIds?: string[];
  /** 由前端生成（crypto.randomUUID），用于关联远程结果与语境事件。 */
  queryId?: string;
}

/** 同步查询本地词典；远程结果通过 `lookup://remote-result` 事件追加。 */
export function lookup(options: LookupOptions): Promise<Entry[]> {
  return call<Entry[]>("lookup", {
    query: options.query,
    sourceIds: options.sourceIds,
    queryId: options.queryId,
  });
}

/** 只保留最近 8 个 query_id 的元数据；过期或未知返回 null。 */
export function getQueryMeta(queryId: string): Promise<QueryMeta | null> {
  return call<QueryMeta | null>("get_query_meta", { queryId });
}

// ---------------------------------------------------------------------------
// 数据源
// ---------------------------------------------------------------------------

/** 仅配置，不含运行时状态。 */
export function listSources(): Promise<SourceConfig[]> {
  return call<SourceConfig[]>("list_sources");
}

/** 保存并异步打开数据源；返回后端生成的完整配置。 */
export function addSource(config: SourceConfig): Promise<SourceConfig> {
  return call<SourceConfig>("add_source", { config });
}

/** 保存并重载数据源。 */
export function updateSource(id: string, config: SourceConfig): Promise<SourceConfig> {
  return call<SourceConfig>("update_source", { id, config });
}

/** 关闭 worker 并删除配置（不删除词典文件）。 */
export function removeSource(id: string): Promise<void> {
  return call<void>("remove_source", { id });
}

/** 运行时状态，含加载/失败信息。 */
export function getSourceStatuses(): Promise<SourceStateEntry[]> {
  return call<SourceStateEntry[]>("get_source_statuses");
}

/** 用草稿配置查询，不写盘、不缓存。 */
export function testSourceQuery(config: SourceConfig, query: string): Promise<TestResult> {
  return call<TestResult>("test_source_query", { config, query });
}

// ---------------------------------------------------------------------------
// Anki
// ---------------------------------------------------------------------------

/** 始终允许重复；同语境提示由 `exact_context_seen` 决定。 */
export function exportToAnki(payload: AnkiPayload): Promise<ExportResult> {
  return call<ExportResult>("export_to_anki", { payload });
}

/** 保存前同词同句同释义检查；只用于提示，不阻断。 */
export function checkCardContext(payload: AnkiPayload): Promise<CardContext> {
  return call<CardContext>("check_card_context", { payload });
}

export function getAnkiDecks(): Promise<string[]> {
  return call<string[]>("get_anki_decks");
}

export function getAnkiModels(): Promise<AnkiModel[]> {
  return call<AnkiModel[]>("get_anki_models");
}

/** 只读探测 AnkiConnect，不写卡。 */
export function probeAnki(): Promise<AnkiStatus> {
  return call<AnkiStatus>("probe_anki");
}

// ---------------------------------------------------------------------------
// 设置与运行时
// ---------------------------------------------------------------------------

/** 返回含桥接令牌的完整配置，UI 需遮罩令牌。 */
export function getSettings(): Promise<Settings> {
  return call<Settings>("get_settings");
}

/** 校验、应用热键并保存；失败时后端不写盘。 */
export function updateSettings(settings: Settings): Promise<Settings> {
  return call<Settings>("update_settings", { settings });
}

export function getRuntimeStatus(): Promise<RuntimeStatus> {
  return call<RuntimeStatus>("get_runtime_status");
}

// ---------------------------------------------------------------------------
// 浏览器扩展桥接
// ---------------------------------------------------------------------------

/** 桥接运行状态：是否监听、端口、最近成功请求与启动错误。 */
export function getBridgeStatus(): Promise<BridgeStatus> {
  return call<BridgeStatus>("get_bridge_status");
}

/** 轮换令牌；返回新令牌，旧令牌立即失效。 */
export function regenerateBridgeToken(): Promise<string> {
  return call<string>("regenerate_bridge_token");
}

/** 打开配置目录（系统文件管理器）。 */
export function openConfigDir(): Promise<void> {
  return call<void>("open_config_dir");
}

// ---------------------------------------------------------------------------
// 窗口
// ---------------------------------------------------------------------------

/** 显示已存在的窗口（不存在时由后端创建）。 */
export function showWindow(label: WindowLabel): Promise<void> {
  return call<void>("show_window", { label });
}

/** 打开白名单窗口，用于弹窗跳转。 */
export function openWindow(target: OpenWindowTarget): Promise<void> {
  return call<void>("open_window", { target });
}

export function hidePopup(): Promise<void> {
  return call<void>("hide_popup");
}

export function setPopupInteractive(interactive: boolean): Promise<void> {
  return call<void>("set_popup_interactive", { interactive });
}

// ---------------------------------------------------------------------------
// M0 调试保留
// ---------------------------------------------------------------------------

export function getCurrentSelection(): Promise<SelectionContext | null> {
  return call<SelectionContext | null>("get_current_selection");
}

/** M0 调试入口：绕过全局热键触发一次捕获。 */
export function debugTriggerCapture(): Promise<CaptureResult> {
  return call<CaptureResult>("debug_trigger_capture");
}

/** 读取最近一次捕获结果（含失败原因），供弹窗恢复错误态。 */
export function getLastCapture(): Promise<CaptureResult | null> {
  return call<CaptureResult | null>("get_last_capture");
}

export function getCaptureStats(): Promise<CaptureStats> {
  return call<CaptureStats>("get_capture_stats");
}
