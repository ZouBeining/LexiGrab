// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import type { KeyboardEvent } from "react";
import { Button } from "../../components/Button";

const NAMED_KEYS: Record<string, string> = {
  " ": "Space",
  Spacebar: "Space",
  Enter: "Enter",
  Tab: "Tab",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta"]);

/** Maps a DOM key to a Tauri accelerator key name; null when unsupported. */
function acceleratorKey(event: KeyboardEvent<HTMLElement>): string | null {
  const { key } = event;
  if (/^F([1-9]|1[0-2])$/.test(key)) return key;
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase();
  if (/^[0-9]$/.test(key)) return key;
  return NAMED_KEYS[key] ?? null;
}

function acceleratorModifiers(event: KeyboardEvent<HTMLElement>): string[] {
  const modifiers: string[] = [];
  // CmdOrCtrl matches the documented default ("CmdOrCtrl+Shift+D").
  if (event.ctrlKey || event.metaKey) modifiers.push("CmdOrCtrl");
  if (event.altKey) modifiers.push("Alt");
  if (event.shiftKey) modifiers.push("Shift");
  return modifiers;
}

export interface HotkeyInputProps {
  id?: string;
  "aria-describedby"?: string;
  "aria-invalid"?: boolean;
  value: string;
  onChange: (accelerator: string) => void;
  disabled?: boolean;
}

/**
 * Keydown recorder for the global hotkey (docs/APP_FLOW.md §7.1).
 * Only complete combinations with at least one modifier are accepted;
 * Esc cancels recording. Value is a Tauri accelerator string.
 */
export function HotkeyInput({
  id,
  "aria-describedby": describedBy,
  "aria-invalid": invalid,
  value,
  onChange,
  disabled = false,
}: HotkeyInputProps) {
  const [recording, setRecording] = useState(false);
  const [hint, setHint] = useState<string | null>(null);

  const startRecording = () => {
    setHint(null);
    setRecording(true);
  };

  const stopRecording = () => {
    setRecording(false);
    setHint(null);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    // While recording, keys must not trigger other shortcuts.
    event.preventDefault();
    event.stopPropagation();

    if (event.key === "Escape") {
      stopRecording();
      return;
    }
    if (MODIFIER_KEYS.has(event.key)) {
      setHint("继续按下一个非修饰键。");
      return;
    }

    const key = acceleratorKey(event);
    if (key === null) {
      setHint("该按键暂不支持，请使用字母、数字、F1–F12 或方向键。");
      return;
    }
    const modifiers = acceleratorModifiers(event);
    if (modifiers.length === 0) {
      setHint("热键必须包含 Ctrl、Alt 或 Shift 修饰键。");
      return;
    }

    onChange([...modifiers, key].join("+"));
    setRecording(false);
    setHint(null);
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-3">
        <Button
          id={id}
          variant="secondary"
          aria-describedby={describedBy}
          aria-invalid={invalid}
          aria-label={recording ? "正在录制全局热键，按 Esc 取消" : "录制全局热键"}
          onClick={recording ? stopRecording : startRecording}
          onKeyDown={onKeyDown}
          onBlur={stopRecording}
          disabled={disabled}
          className="min-w-[180px] justify-start font-mono text-xs tracking-[0.6px]"
        >
          {recording ? "按下组合键…" : value || "尚未设置"}
        </Button>
        <span className="text-xs text-ink-muted">
          {recording ? "按 Esc 取消录制" : "点击后按下组合键，例如 Ctrl+Shift+D"}
        </span>
      </div>
      <p role="status" aria-live="polite" className="min-h-4 text-xs text-ink-muted">
        {hint ?? (recording ? "至少包含一个修饰键。" : "")}
      </p>
    </div>
  );
}
