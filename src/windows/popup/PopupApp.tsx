import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { Button } from "../../components/Button";
import { EmptyState } from "../../components/EmptyState";
import { Spinner } from "../../components/Spinner";
import { SourceBadge } from "../../components/SourceBadge";
import { onHotkeyError, onPopupCaptureError, onPopupSelection } from "../../lib/events";
import {
  checkCardContext,
  exportToAnki,
  getCurrentSelection,
  getSettings,
  hidePopup,
  lookup,
  openWindow,
  setPopupInteractive,
  toIpcError,
} from "../../lib/ipc";
import type {
  AnkiPayload,
  DuplicatePolicy,
  IpcError,
  OpenWindowTarget,
  SelectionContext,
  Settings,
} from "../../lib/types";
import { ContextWarningBar } from "./ContextWarningBar";
import { EditView } from "./EditView";
import { ResultView } from "./ResultView";
import {
  activeEntry,
  buildAnkiPayload,
  displayWord,
  initialState,
  popupReducer,
} from "./state";

const NOT_CAPTURED: IpcError = {
  code: "QUERY_EMPTY",
  message: "未捕获到文本，请重新选择后再试。",
  detail: null,
};

const DEFAULT_DUPLICATE_POLICY: DuplicatePolicy = "warn";

let querySequence = 0;

function createQueryId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  querySequence += 1;
  return `query-${Date.now()}-${querySequence}`;
}

function MoreMenu({
  onOpenManager,
  onOpenSettings,
}: {
  onOpenManager: () => void;
  onOpenSettings: () => void;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target;
      if (target instanceof Node && containerRef.current && !containerRef.current.contains(target)) {
        setOpen(false);
      }
    };
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("mousedown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [open]);

  return (
    <div ref={containerRef} className="relative">
      <Button
        variant="ghost"
        size="md"
        aria-label="更多操作"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="px-3 text-base"
      >
        <span aria-hidden="true">⋯</span>
      </Button>
      {open ? (
        <div
          role="menu"
          className="absolute bottom-11 left-0 z-10 flex w-40 flex-col rounded-xl border border-hairline bg-canvas p-1 shadow-sm"
        >
          <button
            type="button"
            role="menuitem"
            className="rounded-lg px-3 py-2 text-left text-sm text-ink hover:bg-canvas-soft"
            onClick={() => {
              setOpen(false);
              onOpenManager();
            }}
          >
            打开词典管理
          </button>
          <button
            type="button"
            role="menuitem"
            className="rounded-lg px-3 py-2 text-left text-sm text-ink hover:bg-canvas-soft"
            onClick={() => {
              setOpen(false);
              onOpenSettings();
            }}
          >
            设置
          </button>
        </div>
      ) : null}
    </div>
  );
}

function CenteredLoading({ label }: { label: string }) {
  return (
    <div className="flex min-h-[260px] items-center justify-center">
      <Spinner label={label} />
    </div>
  );
}

function ErrorPanel({
  title,
  message,
  retryLabel,
  onRetry,
  onOpenManager,
}: {
  title: string;
  message: string;
  retryLabel: string;
  onRetry: () => void;
  onOpenManager: () => void;
}) {
  return (
    <div
      role="alert"
      className="flex min-h-[260px] flex-col items-center justify-center gap-3 px-4 text-center"
    >
      <p className="text-[16px] font-medium text-danger">{title}</p>
      <p className="max-w-[300px] text-sm leading-5 text-ink-muted">{message}</p>
      <div className="flex flex-wrap items-center justify-center gap-2">
        <Button onClick={onRetry}>{retryLabel}</Button>
        <Button variant="secondary" onClick={onOpenManager}>
          打开词典管理
        </Button>
      </div>
    </div>
  );
}

function SentencePreview({ sentence }: { sentence: string }) {
  return (
    <section className="border-t border-hairline pt-3" aria-label="原句预览">
      <p className="font-mono text-[11px] leading-4 tracking-[0.6px] text-ink-muted">原句</p>
      {sentence.trim().length > 0 ? (
        <p className="mt-1 line-clamp-2 text-sm leading-5 text-ink">{sentence}</p>
      ) : (
        <p className="mt-1 text-sm leading-5 text-ink-muted">未捕获到原句，可在编辑模式补充。</p>
      )}
    </section>
  );
}

function SaveErrorNotice({ error, onOpenSettings }: { error: IpcError; onOpenSettings: () => void }) {
  const unreachable = error.code === "ANKI_UNREACHABLE";
  const fieldMissing = error.code === "ANKI_FIELD_MISSING";
  return (
    <div
      role="alert"
      className="rounded-lg border border-danger/40 px-3 py-2 text-xs leading-5 text-danger"
    >
      <p>{error.message}</p>
      {unreachable ? (
        <p className="mt-1 text-ink-muted">启动 Anki 并安装 AnkiConnect 后重试。</p>
      ) : null}
      {fieldMissing ? (
        <div className="mt-2">
          <Button size="sm" variant="secondary" onClick={onOpenSettings}>
            打开 Anki 设置
          </Button>
        </div>
      ) : null}
    </div>
  );
}

/**
 * Popup orchestrator: capture -> lookup -> results/empty/error, edit mode and the
 * save-to-Anki flow (docs/APP_FLOW.md §6, docs/BACKEND_STRUCTURE.md §3/§4).
 *
 * Side effects live here; all state transitions go through the pure reducer.
 */
export function PopupApp() {
  const [state, dispatch] = useReducer(popupReducer, initialState);
  const stateRef = useRef(state);
  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const settingsRef = useRef<Settings | null>(null);
  /** Text of the selection already displayed; a same-text event only refreshes the sentence. */
  const selectionTextRef = useRef<string | null>(null);
  /** Incremented on close/new lookup so late async completions cannot update a stale popup. */
  const operationRef = useRef(0);

  const startLookup = useCallback((selection: SelectionContext) => {
    operationRef.current += 1;
    const queryId = createQueryId();
    selectionTextRef.current = selection.text;
    dispatch({ type: "selection-received", selection, queryId });
    lookup({ query: selection.text, queryId })
      .then((entries) => dispatch({ type: "lookup-succeeded", queryId, entries }))
      .catch((error: unknown) =>
        dispatch({ type: "lookup-failed", queryId, error: toIpcError(error) }),
      );
  }, []);

  const handleSelection = useCallback(
    (selection: SelectionContext) => {
      // The backend re-sends popup://selection with the sentence filled in; keep state.
      if (selectionTextRef.current === selection.text) {
        dispatch({ type: "selection-context-updated", selection });
        return;
      }
      startLookup(selection);
    },
    [startLookup],
  );

  useEffect(() => {
    let active = true;
    const stops: Array<() => void> = [];
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
          // Subscription failures fall back to the mount-time getCurrentSelection fetch.
        });
    };

    getSettings()
      .then((settings) => {
        if (active) {
          settingsRef.current = settings;
        }
      })
      .catch(() => {
        // Keep the safer "warn" policy when settings cannot be read.
      });

    subscribe(onPopupSelection(handleSelection));
    subscribe(
      onPopupCaptureError((error) => {
        selectionTextRef.current = null;
        dispatch({ type: "capture-failed", error });
      }),
    );
    subscribe(
      onHotkeyError(() => {
        // Hotkey conflicts are surfaced in Settings, not in the popup.
      }),
    );

    // Events may fire before the listener is ready; always fetch once as a fallback.
    getCurrentSelection()
      .then((selection) => {
        if (!active) {
          return;
        }
        if (selection) {
          handleSelection(selection);
        } else if (selectionTextRef.current === null && stateRef.current.phase === "capturing") {
          dispatch({ type: "capture-failed", error: NOT_CAPTURED });
        }
      })
      .catch((error: unknown) => {
        if (active) {
          dispatch({ type: "capture-failed", error: toIpcError(error) });
        }
      });

    return () => {
      active = false;
      for (const stop of stops) {
        stop();
      }
    };
  }, [handleSelection]);

  const handleClose = useCallback(() => {
    operationRef.current += 1;
    selectionTextRef.current = null;
    dispatch({ type: "reset" });
    void hidePopup().catch(() => {
      // The window is already going away; nothing actionable for the UI.
    });
  }, []);

  const openTarget = useCallback((target: OpenWindowTarget) => {
    operationRef.current += 1;
    selectionTextRef.current = null;
    dispatch({ type: "reset" });
    void openWindow(target).catch(() => {
      // Navigation failures keep the popup state intact from the user's perspective.
    });
    void hidePopup().catch(() => {
      // Hiding is best-effort; openWindow already asked the backend to focus the target.
    });
  }, []);

  const handleRetryCapture = useCallback(() => {
    selectionTextRef.current = null;
    dispatch({ type: "reset" });
    getCurrentSelection()
      .then((selection) => {
        if (selection) {
          handleSelection(selection);
        } else {
          dispatch({ type: "capture-failed", error: NOT_CAPTURED });
        }
      })
      .catch((error: unknown) => dispatch({ type: "capture-failed", error: toIpcError(error) }));
  }, [handleSelection]);

  const handleRetryLookup = useCallback(() => {
    const selection = stateRef.current.selection;
    if (selection) {
      startLookup(selection);
    }
  }, [startLookup]);

  const duplicatePolicy = useCallback((): DuplicatePolicy => {
    return settingsRef.current?.anki.duplicate_policy ?? DEFAULT_DUPLICATE_POLICY;
  }, []);

  const runExport = useCallback(async (payload: AnkiPayload, operation: number) => {
    try {
      const result = await exportToAnki(payload);
      if (operationRef.current !== operation) {
        return;
      }
      dispatch({ type: "save-succeeded", result });
    } catch (error: unknown) {
      if (operationRef.current !== operation) {
        return;
      }
      dispatch({ type: "save-failed", error: toIpcError(error) });
    }
  }, []);

  const handleSave = useCallback(async () => {
    const snapshot = stateRef.current;
    if (
      snapshot.phase !== "results" &&
      snapshot.phase !== "empty" &&
      snapshot.phase !== "editing" &&
      snapshot.phase !== "saveError"
    ) {
      return;
    }
    const operation = operationRef.current + 1;
    operationRef.current = operation;
    const payload = buildAnkiPayload(snapshot);
    dispatch({ type: "save-requested" });

    if (duplicatePolicy() === "warn") {
      try {
        const context = await checkCardContext(payload);
        if (operationRef.current !== operation) {
          return;
        }
        if (context.seen) {
          dispatch({ type: "context-warning", warning: context });
          return;
        }
      } catch {
        if (operationRef.current !== operation) {
          return;
        }
        // The context check only drives a hint; a failed check must not block export.
      }
    }

    dispatch({ type: "save-confirmed" });
    await runExport(payload, operation);
  }, [duplicatePolicy, runExport]);

  const handleSaveAnyway = useCallback(async () => {
    const operation = operationRef.current + 1;
    operationRef.current = operation;
    const payload = buildAnkiPayload(stateRef.current);
    dispatch({ type: "save-confirmed" });
    await runExport(payload, operation);
  }, [runExport]);

  const handleEdit = useCallback(() => {
    void setPopupInteractive(true).catch(() => {
      // Editing still renders; the backend retries window activation.
    });
    dispatch({ type: "edit-requested" });
  }, []);

  const handleCancelEdit = useCallback(() => {
    // Per Lead decision: do not call setPopupInteractive(false) — it hides the window.
    dispatch({ type: "edit-cancelled" });
  }, []);

  const handleSwitchEntry = useCallback(() => {
    if (stateRef.current.phase === "editing") {
      dispatch({ type: "edit-cancelled" });
      return;
    }
    dispatch({ type: "context-warning-dismissed" });
  }, []);

  const handleBackToResults = useCallback(() => {
    dispatch({ type: "back-to-results" });
  }, []);

  const handleSelectEntry = useCallback((index: number) => {
    dispatch({ type: "entry-selected", index });
  }, []);

  const handleDraftChange = useCallback((field: "word" | "definition" | "sentence", value: string) => {
    dispatch({ type: "draft-changed", field, value });
  }, []);

  const entry = activeEntry(state);
  const word = displayWord(state);
  const sourceName = entry?.source_name ?? "";
  const isSaving = state.phase === "saving";
  const showEditor =
    state.phase === "editing" ||
    (state.saveOrigin === "editing" && (state.phase === "saving" || state.phase === "saveError"));

  if (showEditor) {
    return (
      <EditView
        word={word}
        source={sourceName}
        definition={state.draftDefinition}
        sentence={state.draftSentence}
        error={state.error}
        contextWarning={state.contextWarning}
        saving={isSaving}
        onChange={handleDraftChange}
        onSave={() => void handleSave()}
        onSaveAnyway={() => void handleSaveAnyway()}
        onCancel={handleCancelEdit}
        onSwitchEntry={handleSwitchEntry}
      />
    );
  }

  const renderBody = () => {
    switch (state.phase) {
      case "capturing":
        return <CenteredLoading label="正在读取选区…" />;
      case "loading":
        return <CenteredLoading label="正在查询释义…" />;
      case "captureError":
        return (
          <ErrorPanel
            title="未捕获到文本"
            message={state.error?.message ?? NOT_CAPTURED.message}
            retryLabel="重新捕获"
            onRetry={handleRetryCapture}
            onOpenManager={() => openTarget("manager")}
          />
        );
      case "queryError":
        return (
          <ErrorPanel
            title="查询失败"
            message={state.error?.message ?? "词典没有返回结果，请稍后重试。"}
            retryLabel="重试"
            onRetry={handleRetryLookup}
            onOpenManager={() => openTarget("manager")}
          />
        );
      case "empty":
        return (
          <div className="flex min-h-[300px] items-center justify-center">
            <EmptyState
              title="未找到释义"
              description="当前词典中没有这个单词。可以补充释义后制卡，或先添加词典。"
              action={
                <>
                  <Button onClick={handleEdit}>仍然制卡</Button>
                  <Button variant="secondary" onClick={() => openTarget("manager")}>
                    添加词典
                  </Button>
                </>
              }
            />
          </div>
        );
      default:
        return renderResults();
    }
  };

  const renderResults = () => {
    const saved = state.phase === "saved";
    const failed = state.phase === "saveError" && state.error !== null;
    return (
      <div className="flex flex-col gap-3">
        {isSaving ? (
          <div className="flex items-center gap-2 rounded-lg bg-canvas-soft px-3 py-2">
            <Spinner size="sm" />
            <span className="text-xs text-ink-muted">正在保存到 Anki…</span>
          </div>
        ) : null}

        {saved && state.saveResult ? (
          <div className="rounded-lg border border-success/40 bg-canvas-soft px-3 py-2 text-xs leading-5">
            <p className="font-medium text-success">已保存到 Anki</p>
            <p className="mt-0.5 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              note {state.saveResult.note_id}
            </p>
            {state.saveResult.exact_context_seen ? (
              <p className="mt-1 text-ink-muted">此语境之前保存过，本次已写入一张新卡片。</p>
            ) : null}
          </div>
        ) : null}

        {failed && state.error ? (
          <SaveErrorNotice error={state.error} onOpenSettings={() => openTarget("settings")} />
        ) : null}

        <div className="max-h-[300px] overflow-y-auto pr-1">
          {state.entries.length > 0 ? (
            <ResultView
              entries={state.entries}
              selectedEntryIndex={state.selectedEntryIndex}
              recommendedSenseId={null}
              onSelectEntry={handleSelectEntry}
            />
          ) : (
            <p className="text-sm leading-5 text-ink-muted">没有可保存的释义，进入编辑模式补充后再保存。</p>
          )}
        </div>

        <SentencePreview sentence={state.draftSentence} />
      </div>
    );
  };

  const renderFooterActions = () => {
    switch (state.phase) {
      case "results":
        return (
          <>
            <Button variant="secondary" onClick={handleEdit}>
              编辑
            </Button>
            <Button onClick={() => void handleSave()}>保存所选义项</Button>
          </>
        );
      case "saving":
        return (
          <>
            <Button variant="secondary" disabled>
              编辑
            </Button>
            <Button disabled>保存中…</Button>
          </>
        );
      case "saved":
        return (
          <>
            <Button variant="secondary" onClick={handleClose}>
              关闭
            </Button>
            <Button onClick={handleBackToResults}>再存一张</Button>
          </>
        );
      case "saveError":
        return (
          <>
            <Button variant="secondary" onClick={handleEdit} disabled={isSaving}>
              编辑
            </Button>
            <Button onClick={() => void handleSave()} disabled={isSaving}>
              重试保存
            </Button>
          </>
        );
      default:
        return (
          <Button variant="secondary" onClick={handleClose}>
            关闭
          </Button>
        );
    }
  };

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-hairline bg-canvas">
      <header className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-hairline px-4">
        <div className="flex min-w-0 items-baseline gap-2">
          <h1 className="truncate text-[24px] font-bold leading-tight tracking-tight text-ink">
            {word || "LexiGrab"}
          </h1>
          {entry?.phonetic ? (
            <span className="truncate font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {entry.phonetic}
            </span>
          ) : null}
          {sourceName ? <SourceBadge name={sourceName} className="shrink-0" /> : null}
        </div>
        <button
          type="button"
          onClick={handleClose}
          aria-label="关闭弹窗"
          className="flex size-10 shrink-0 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 hover:bg-canvas-soft hover:text-ink active:scale-[0.98]"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">{renderBody()}</div>

      {state.contextWarning ? (
        <ContextWarningBar
          warning={state.contextWarning}
          disabled={isSaving}
          onSaveAnyway={() => void handleSaveAnyway()}
          onSwitchEntry={handleSwitchEntry}
        />
      ) : null}

      <footer className="flex h-14 shrink-0 items-center justify-between gap-2 border-t border-hairline px-4">
        <MoreMenu
          onOpenManager={() => openTarget("manager")}
          onOpenSettings={() => openTarget("settings")}
        />
        <div className="flex items-center gap-2">{renderFooterActions()}</div>
      </footer>
    </div>
  );
}
