import { invoke } from "@tauri-apps/api/core";
import type {
  CaptureResult,
  CaptureStats,
  IpcError,
  RuntimeStatus,
  SelectionContext,
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

/** 显示已存在的窗口（不存在时由后端创建）。 */
export function showWindow(label: WindowLabel): Promise<void> {
  return call<void>("show_window", { label });
}

/** 打开白名单窗口，用于弹窗跳转。 */
export function openWindow(target: "manager" | "settings"): Promise<void> {
  return call<void>("open_window", { target });
}

export function hidePopup(): Promise<void> {
  return call<void>("hide_popup");
}

export function setPopupInteractive(interactive: boolean): Promise<void> {
  return call<void>("set_popup_interactive", { interactive });
}

export function getCurrentSelection(): Promise<SelectionContext | null> {
  return call<SelectionContext | null>("get_current_selection");
}

/** M0 调试入口：绕过全局热键触发一次捕获。 */
export function debugTriggerCapture(): Promise<CaptureResult> {
  return call<CaptureResult>("debug_trigger_capture");
}

/** M0 专用：读取最近一次捕获结果（含失败原因），供弹窗恢复错误态。 */
export function getLastCapture(): Promise<CaptureResult | null> {
  return call<CaptureResult | null>("get_last_capture");
}

export function getCaptureStats(): Promise<CaptureStats> {
  return call<CaptureStats>("get_capture_stats");
}

export function getRuntimeStatus(): Promise<RuntimeStatus> {
  return call<RuntimeStatus>("get_runtime_status");
}
