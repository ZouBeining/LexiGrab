import { listen } from "@tauri-apps/api/event";
import type { CaptureError, SelectionContext } from "./types";

/** 事件名集中定义，与 docs/BACKEND_STRUCTURE.md 4.2 一致；M0 增加捕获失败事件。 */
export const EVENTS = {
  popupSelection: "popup://selection",
  popupCaptureError: "popup://capture-error",
  hotkeyError: "runtime://hotkey-error",
  captureComplete: "capture://complete",
} as const;

export function onPopupSelection(handler: (payload: SelectionContext) => void): Promise<() => void> {
  return listen<SelectionContext>(EVENTS.popupSelection, (event) => handler(event.payload));
}

export function onPopupCaptureError(handler: (payload: CaptureError) => void): Promise<() => void> {
  return listen<CaptureError>(EVENTS.popupCaptureError, (event) => handler(event.payload));
}

export function onHotkeyError(handler: (payload: { message: string }) => void): Promise<() => void> {
  return listen<{ message: string }>(EVENTS.hotkeyError, (event) => handler(event.payload));
}
