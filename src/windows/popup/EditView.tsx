// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { Button } from "../../components/Button";
import { Field } from "../../components/Field";
import { SourceBadge } from "../../components/SourceBadge";
import type { CardContext, IpcError } from "../../lib/types";
import { ContextWarningBar } from "./ContextWarningBar";
import { DefinitionView } from "./DefinitionView";
import type { DraftField } from "./state";

const INPUT_CLASS =
  "w-full rounded-lg border border-hairline bg-canvas px-3 py-2 text-sm text-ink read-only:bg-canvas-soft";

const TEXTAREA_CLASS = `${INPUT_CLASS} min-h-[88px] resize-y leading-6`;

export type EditorTab = "preview" | "source";

export interface EditViewProps {
  word: string;
  source: string;
  definition: string;
  sentence: string;
  error: IpcError | null;
  /** Same-context hint raised by the last save attempt, if any. */
  contextWarning: CardContext | null;
  /** True while exportToAnki is in flight; inputs are disabled but kept. */
  saving: boolean;
  onChange: (field: DraftField, value: string) => void;
  onSave: () => void;
  onSaveAnyway: () => void;
  onCancel: () => void;
  onSwitchEntry: () => void;
}

/**
 * Edit mode layout (docs/APP_FLOW.md §6.4): 560×640, preview/source tabs,
 * editable sentence, read-only word/source, Ctrl+Enter saves and Esc cancels.
 */
export function EditView({
  word,
  source,
  definition,
  sentence,
  error,
  contextWarning,
  saving,
  onChange,
  onSave,
  onSaveAnyway,
  onCancel,
  onSwitchEntry,
}: EditViewProps) {
  const [tab, setTab] = useState<EditorTab>("preview");

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (!saving) onCancel();
        return;
      }
      if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
        event.preventDefault();
        if (!saving) onSave();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onCancel, onSave, saving]);

  const tabClass = (active: boolean) =>
    [
      "h-7 rounded-full px-3 text-xs transition-colors duration-150",
      active ? "bg-accent text-on-accent" : "text-ink-muted hover:bg-canvas-soft hover:text-ink",
    ].join(" ");

  return (
    <div className="flex h-full flex-col overflow-hidden rounded-xl border border-hairline bg-canvas">
      <header className="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-hairline px-4">
        <div className="flex min-w-0 items-baseline gap-2">
          <span className="truncate text-[15px] font-semibold leading-6 text-ink">
            {word || "编辑卡片"}
          </span>
          <SourceBadge name={source || "未知来源"} />
        </div>
        <span className="shrink-0 font-mono text-[11px] tracking-[0.6px] text-ink-muted">编辑模式</span>
      </header>

      <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
        {error ? (
          <div
            role="alert"
            className="flex items-start gap-2 rounded-lg border border-danger/40 px-3 py-2 text-xs leading-5 text-danger"
          >
            <span aria-hidden="true">⚠</span>
            <div className="min-w-0">
              <p>{error.message}</p>
              {error.detail ? (
                <p className="mt-1 break-all font-mono text-[11px] text-ink-muted">{error.detail}</p>
              ) : null}
            </div>
          </div>
        ) : null}

        <div className="grid grid-cols-2 gap-3">
          <Field label="单词" hint="只读">
            <input readOnly value={word} className={INPUT_CLASS} />
          </Field>
          <Field label="来源" hint="只读">
            <input readOnly value={source || "未知来源"} className={INPUT_CLASS} />
          </Field>
        </div>

        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <div role="tablist" aria-label="释义显示方式" className="inline-flex gap-0.5 rounded-full border border-hairline p-0.5">
              <button
                type="button"
                role="tab"
                aria-selected={tab === "preview"}
                aria-controls="definition-panel"
                onClick={() => setTab("preview")}
                className={tabClass(tab === "preview")}
              >
                预览
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={tab === "source"}
                aria-controls="definition-panel"
                onClick={() => setTab("source")}
                className={tabClass(tab === "source")}
              >
                源码
              </button>
            </div>
            <span className="text-[11px] leading-4 text-ink-muted">保存内容以源码为准</span>
          </div>

          {tab === "preview" ? (
            <div
              id="definition-panel"
              role="tabpanel"
              aria-label="释义预览"
              className="min-h-[160px] rounded-lg border border-hairline bg-canvas-soft p-3"
            >
              {definition.trim() ? (
                <DefinitionView html={definition} />
              ) : (
                <p className="text-sm text-ink-muted">释义为空，可切换到“源码”补充。</p>
              )}
            </div>
          ) : (
            <div id="definition-panel" role="tabpanel" aria-label="释义源码">
              <Field label="释义源码（HTML）" hint="支持基础 HTML；导出时本地图片与音视频会被替换为占位文本。">
                <textarea
                  value={definition}
                  onChange={(event) => onChange("definition", event.target.value)}
                  className={TEXTAREA_CLASS}
                  rows={7}
                  spellCheck={false}
                  disabled={saving}
                />
              </Field>
            </div>
          )}
        </div>

        <Field label="原句" hint="允许为空；保存时写入 Anki 的 Sentence 字段。">
          <textarea
            value={sentence}
            onChange={(event) => onChange("sentence", event.target.value)}
            className={TEXTAREA_CLASS}
            rows={3}
            placeholder="未捕获到原句，可在此补充。"
            disabled={saving}
          />
        </Field>
      </div>

      {contextWarning ? (
        <ContextWarningBar
          warning={contextWarning}
          disabled={saving}
          onSaveAnyway={onSaveAnyway}
          onSwitchEntry={onSwitchEntry}
        />
      ) : null}

      <footer className="flex h-14 shrink-0 items-center justify-between gap-2 border-t border-hairline px-4">
        <span className="text-[11px] leading-4 text-ink-muted">Ctrl+Enter 保存 · Esc 取消</span>
        <div className="flex items-center gap-2">
          <Button variant="secondary" onClick={onCancel} disabled={saving}>
            取消
          </Button>
          <Button onClick={onSave} disabled={saving}>
            {saving ? "保存中…" : "保存卡片"}
          </Button>
        </div>
      </footer>
    </div>
  );
}
