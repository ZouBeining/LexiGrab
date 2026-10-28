/** IPC DTO。字段与 docs/BACKEND_STRUCTURE.md 第 2、4 节保持一致；snake_case 对齐 Rust serde。 */

export type CaptureMethod = "clipboard" | "uia" | "manual";

export interface ScreenAnchor {
  x: number;
  y: number;
  dpi_scale: number;
}

export interface SelectionContext {
  text: string;
  anchor: ScreenAnchor | null;
  foreground_hwnd: number;
  app_name: string;
  method: CaptureMethod;
  captured_at_ms: number;
  elapsed_ms: number;
}

export interface IpcError {
  code: string;
  message: string;
  detail: string | null;
}

export interface HotkeyStatus {
  accelerator: string;
  registered: boolean;
  message: string | null;
}

export interface RuntimeStatus {
  version: string;
  hotkey: HotkeyStatus;
  config_dir: string;
  log_dir: string;
  popup_visible: boolean;
}

export interface CaptureError {
  code: string;
  message: string;
  detail: string | null;
}

export interface CaptureResult {
  selection: SelectionContext | null;
  error: CaptureError | null;
}

export interface CaptureSample {
  elapsed_ms: number;
  method: CaptureMethod;
  app_name: string;
  succeeded: boolean;
}

export interface CaptureStats {
  samples: CaptureSample[];
  p50_ms: number | null;
  p95_ms: number | null;
  success_rate: number | null;
}

export type WindowLabel = "manager" | "settings" | "popup";
