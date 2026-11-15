/**
 * Tauri 事件名与载荷类型集中在此，与 docs/BACKEND_STRUCTURE.md §4.2 一致。
 * 组件不手写事件字符串，一律用 EVENTS 常量或 listen* 辅助函数。
 */
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AnkiStatus,
  BridgeStatus,
  CaptureError,
  Entry,
  SelectionContext,
  SenseResolution,
} from "./types";

/** 事件 wire 名。 */
export const EVENTS = {
  popupSelection: "popup://selection",
  /** M0 保留：捕获失败。 */
  popupCaptureError: "popup://capture-error",
  /** 编辑模式激活、输入框可聚焦后触发，无载荷。 */
  popupEditReady: "popup://edit-ready",
  dictIndexProgress: "dict://index-progress",
  hotkeyError: "runtime://hotkey-error",
  ankiStatus: "runtime://anki-status",
  bridgeStatus: "bridge://status",
  remoteResult: "lookup://remote-result",
  remoteError: "lookup://remote-error",
  senseResult: "lookup://sense-result",
} as const;

export type EventName = (typeof EVENTS)[keyof typeof EVENTS];

// ---------------------------------------------------------------------------
// 载荷类型
// ---------------------------------------------------------------------------

/** 词典打开/索引进度；`entry_count` 与 `message` 视 phase 而定。 */
export interface DictIndexProgress {
  source_id: string;
  path: string;
  phase: string;
  entry_count?: number | null;
  message?: string | null;
}

/** 远程源异步追加的结果。 */
export interface RemoteLookupResult {
  query_id: string;
  source_id: string;
  source_name: string;
  entries: Entry[];
  cached: boolean;
  elapsed_ms: number;
}

/** 远程源失败；不弹阻断错误，只做提示。 */
export interface RemoteLookupError {
  query_id: string;
  source_id: string;
  code: string;
  message: string;
}

export interface HotkeyErrorPayload {
  message: string;
}

// ---------------------------------------------------------------------------
// 订阅辅助。返回 Promise<UnlistenFn>，组件卸载时调用以取消订阅。
// ---------------------------------------------------------------------------

export function onPopupSelection(handler: (payload: SelectionContext) => void): Promise<UnlistenFn> {
  return listen<SelectionContext>(EVENTS.popupSelection, (event) => handler(event.payload));
}

export function onPopupCaptureError(handler: (payload: CaptureError) => void): Promise<UnlistenFn> {
  return listen<CaptureError>(EVENTS.popupCaptureError, (event) => handler(event.payload));
}

/** 无载荷事件；handler 在每次触发时调用。 */
export function onPopupEditReady(handler: () => void): Promise<UnlistenFn> {
  return listen(EVENTS.popupEditReady, () => handler());
}

export function onDictIndexProgress(
  handler: (payload: DictIndexProgress) => void,
): Promise<UnlistenFn> {
  return listen<DictIndexProgress>(EVENTS.dictIndexProgress, (event) => handler(event.payload));
}

export function onHotkeyError(handler: (payload: HotkeyErrorPayload) => void): Promise<UnlistenFn> {
  return listen<HotkeyErrorPayload>(EVENTS.hotkeyError, (event) => handler(event.payload));
}

export function onAnkiStatus(handler: (payload: AnkiStatus) => void): Promise<UnlistenFn> {
  return listen<AnkiStatus>(EVENTS.ankiStatus, (event) => handler(event.payload));
}

/** 桥接启停、端口变化或启动失败时触发。 */
export function onBridgeStatus(handler: (payload: BridgeStatus) => void): Promise<UnlistenFn> {
  return listen<BridgeStatus>(EVENTS.bridgeStatus, (event) => handler(event.payload));
}

export function onRemoteLookupResult(
  handler: (payload: RemoteLookupResult) => void,
): Promise<UnlistenFn> {
  return listen<RemoteLookupResult>(EVENTS.remoteResult, (event) => handler(event.payload));
}

export function onRemoteLookupError(
  handler: (payload: RemoteLookupError) => void,
): Promise<UnlistenFn> {
  return listen<RemoteLookupError>(EVENTS.remoteError, (event) => handler(event.payload));
}

export function onSenseResult(handler: (payload: SenseResolution) => void): Promise<UnlistenFn> {
  return listen<SenseResolution>(EVENTS.senseResult, (event) => handler(event.payload));
}
