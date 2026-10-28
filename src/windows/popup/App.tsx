import { useCallback, useEffect, useState } from "react";
import { onPopupCaptureError, onPopupSelection } from "../../lib/events";
import {
  getCurrentSelection,
  getLastCapture,
  hidePopup,
  setPopupInteractive,
  toIpcError,
} from "../../lib/ipc";
import type { IpcError, SelectionContext } from "../../lib/types";

type PopupState =
  | { phase: "loading" }
  | { phase: "ready"; selection: SelectionContext }
  | { phase: "empty" }
  | { phase: "error"; error: IpcError };

function Header({ title, onClose }: { title: string; onClose: () => void }) {
  return (
    <header className="flex h-11 shrink-0 items-center justify-between border-b border-hairline px-4">
      <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">{title}</span>
      <button
        type="button"
        onClick={onClose}
        aria-label="关闭弹窗"
        className="flex size-8 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 hover:bg-canvas-soft hover:text-ink"
      >
        <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
          <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
      </button>
    </header>
  );
}

function PrimaryButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="h-10 rounded-full bg-accent px-5 text-sm text-on-accent transition-transform duration-150 active:scale-[0.98]"
    >
      {label}
    </button>
  );
}

function SecondaryButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="h-10 rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft active:bg-canvas-soft"
    >
      {label}
    </button>
  );
}

export function App() {
  const [state, setState] = useState<PopupState>({ phase: "loading" });

  const fetchSelection = useCallback(() => {
    getCurrentSelection()
      .then(async (selection) => {
        if (selection) {
          setState({ phase: "ready", selection });
          return;
        }
        const last = await getLastCapture();
        if (last?.error) {
          setState({ phase: "error", error: last.error });
        } else {
          setState({ phase: "empty" });
        }
      })
      .catch((cause: unknown) => setState({ phase: "error", error: toIpcError(cause) }));
  }, []);

  const reload = () => {
    setState({ phase: "loading" });
    fetchSelection();
  };

  useEffect(() => {
    fetchSelection();
    const stops: Array<() => void> = [];
    let active = true;

    const subscribe = (promise: Promise<() => void>) => {
      promise
        .then((stop) => {
          if (active) {
            stops.push(stop);
          } else {
            stop();
          }
        })
        .catch(() => {
          // 事件订阅失败时仍可手动刷新。
        });
    };

    subscribe(onPopupSelection((selection) => setState({ phase: "ready", selection })));
    subscribe(onPopupCaptureError((error) => setState({ phase: "error", error })));

    return () => {
      active = false;
      for (const stop of stops) {
        stop();
      }
    };
  }, [fetchSelection]);

  const close = () => {
    void hidePopup();
  };

  const enterEdit = () => {
    void setPopupInteractive(true);
  };

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-hairline bg-canvas">
      <Header title="LexiGrab 划词" onClose={close} />

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        {state.phase === "loading" ? (
          <div role="status" className="flex h-full items-center justify-center text-sm text-ink-muted">
            正在读取选区…
          </div>
        ) : state.phase === "empty" ? (
          <div className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-sm font-medium">没有捕获到文本</p>
            <p className="max-w-[280px] text-xs text-ink-muted">
              在任意应用选中文字后按 Ctrl+Shift+D，或点击下方重新捕获。
            </p>
          </div>
        ) : state.phase === "error" ? (
          <div role="alert" className="flex h-full flex-col items-center justify-center gap-2 text-center">
            <p className="text-sm font-medium text-danger">{state.error.message}</p>
            <p className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">{state.error.code}</p>
          </div>
        ) : (
          <div className="flex flex-col gap-5">
            <div>
              <h1 className="text-[24px] font-bold leading-tight tracking-tight break-words">
                {state.selection.text}
              </h1>
              <p className="mt-2 flex flex-wrap items-center gap-2 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                <span>{state.selection.app_name}</span>
                <span aria-hidden="true">·</span>
                <span>{state.selection.method}</span>
                <span aria-hidden="true">·</span>
                <span>{Math.round(state.selection.elapsed_ms)} ms</span>
              </p>
            </div>

            <div className="rounded-2xl bg-canvas-soft px-4 py-3 text-sm text-ink-muted">
              M0 阶段只验证选区捕获与非激活弹窗，释义、原句与 Anki 在 M1/M2 接入。
            </div>
          </div>
        )}
      </div>

      <footer className="flex h-14 shrink-0 items-center justify-between gap-2 border-t border-hairline px-4">
        <div className="flex items-center gap-2">
          <SecondaryButton label="重新捕获" onClick={reload} />
        </div>
        <div className="flex items-center gap-2">
          <SecondaryButton label="编辑" onClick={enterEdit} />
          <PrimaryButton label="关闭" onClick={close} />
        </div>
      </footer>
    </div>
  );
}
