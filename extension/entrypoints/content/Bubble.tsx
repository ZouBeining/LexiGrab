// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useMemo, useState } from "react";
import type {
  AnkiPayload,
  AnkiResult,
  BridgeEntry,
  SelectionPayload,
  SelectionResult,
  SenseResolutionView,
  SenseResult,
} from "../../lib/bridge";
import type { AnchorRect, PageMeta } from "../../lib/dom";
import type { RuntimeRequest } from "../../lib/messages";
import { sanitizeDefinitionHtml } from "../../lib/sanitize";
import type { SentenceSnapshot } from "../../lib/sentence";

/** 气泡定位结果，由 content script 计算（含上方翻转）。 */
export interface BubblePosition {
  left: number;
  top: number;
  placement: "below" | "above";
}

/** content script 传给气泡的完整快照。 */
export interface BubbleSnapshot extends SentenceSnapshot, PageMeta {}

export type SendFn = (request: RuntimeRequest) => Promise<unknown>;

export interface BubbleProps {
  snapshot: BubbleSnapshot;
  position: BubblePosition;
  send: SendFn;
  onClose: () => void;
}

type Phase = "loading" | "results" | "empty" | "error" | "saved";

interface Failure {
  code: string;
  message: string;
}

/** 结果最多展示 3 条（APP_FLOW §8.2）。 */
const MAX_ENTRIES = 3;
/** 语境轮询 1s 一次，最多 5 次，不阻塞首屏。 */
const SENSE_INTERVAL_MS = 1000;
const SENSE_MAX_ATTEMPTS = 5;

const ERROR_COPY: Record<string, { title: string; action: string }> = {
  NOT_CONFIGURED: { title: "还没有配对", action: "打开扩展设置，填入桌面端的端口与令牌后测试连接。" },
  DESKTOP_OFFLINE: { title: "桌面端未连接", action: "启动 LexiGrab，并在桌面端设置里确认桥接已启用。" },
  AUTH_FAILED: {
    title: "令牌错误",
    action: "从桌面端“设置 → 浏览器扩展”重新复制令牌，再点“测试连接”。",
  },
  ORIGIN_REJECTED: { title: "来源被拒绝", action: "请使用 Firefox 加载本扩展后再试。" },
  TIMEOUT: { title: "请求超时", action: "重试一次；词典可能正在加载。" },
  SERVER_ERROR: { title: "桌面端处理失败", action: "稍后重试，或查看桌面端日志。" },
  INVALID_RESPONSE: { title: "响应无法解析", action: "确认扩展与桌面端版本一致。" },
  ANKI_UNREACHABLE: {
    title: "Anki 未连接",
    action: "启动 Anki 并确认 AnkiConnect 已安装，然后重试保存。",
  },
  ANKI_API_ERROR: { title: "Anki 返回错误", action: "查看桌面端日志后重试。" },
  ANKI_FIELD_MISSING: {
    title: "卡片模板缺少字段",
    action: "在桌面端设置里核对字段映射，或修改 Anki 模板。",
  },
  ANKI_DUPLICATE: {
    title: "Anki 拒绝重复卡片",
    action: "确认 AnkiConnect 允许重复；应用已始终请求 allowDuplicate。",
  },
  RATE_LIMITED: { title: "请求过于频繁", action: "稍等一秒再试。" },
  PAYLOAD_TOO_LARGE: { title: "内容过大", action: "缩短释义或换一个义项再保存。" },
  INVALID_REQUEST: { title: "请求不完整", action: "重新划词后再试一次。" },
};

function toFailure(cause: unknown): Failure {
  if (cause && typeof cause === "object" && "code" in cause && "message" in cause) {
    const candidate = cause as { code: unknown; message: unknown };
    return {
      code: typeof candidate.code === "string" ? candidate.code : "SERVER_ERROR",
      message: typeof candidate.message === "string" ? candidate.message : "请求失败",
    };
  }
  return { code: "SERVER_ERROR", message: "请求失败，请重试" };
}

function plainText(html: string): string {
  const container = document.createElement("div");
  container.innerHTML = html;
  return (container.textContent ?? "").replace(/\s+/g, " ").trim();
}

function sanitize(html: string): string {
  return sanitizeDefinitionHtml(html);
}

function confidenceText(value: number | null): string | null {
  if (value === null || Number.isNaN(value)) return null;
  return `${Math.round(value * 100)}%`;
}

/**
 * 结果气泡（docs/APP_FLOW.md §8.2）：loading → results/empty/error，
 * 可选轮询语境推荐，保存后提供“再存一张”，同词保存不阻断。
 */
export function Bubble({ snapshot, position, send, onClose }: BubbleProps) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [entries, setEntries] = useState<BridgeEntry[]>([]);
  const [sourceNames, setSourceNames] = useState<string[]>([]);
  const [partial, setPartial] = useState(false);
  const [selected, setSelected] = useState(0);
  const [queryId, setQueryId] = useState<string | null>(null);
  const [senseAvailable, setSenseAvailable] = useState(false);
  const [sense, setSense] = useState<SenseResolutionView | null>(null);
  const [error, setError] = useState<Failure | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<Failure | null>(null);
  const [savedNoteId, setSavedNoteId] = useState<number | null>(null);
  const [exactContextSeen, setExactContextSeen] = useState(false);

  const payload: SelectionPayload = useMemo(
    () => ({
      word: snapshot.word,
      sentence: snapshot.sentence,
      url: snapshot.url,
      title: snapshot.title,
      domain: snapshot.domain,
    }),
    [snapshot],
  );

  useEffect(() => {
    let cancelled = false;
    setPhase("loading");
    setError(null);
    send({ type: "lexigrab:selection", payload })
      .then((raw) => {
        if (cancelled) return;
        const result = raw as SelectionResult;
        setQueryId(result.query_id);
        setSenseAvailable(result.sense_available === true);
        setSourceNames(Array.isArray(result.source_names) ? result.source_names : []);
        setPartial(result.partial === true);
        setEntries(Array.isArray(result.entries) ? result.entries : []);
        setSelected(0);
        setPhase(result.entries.length === 0 ? "empty" : "results");
      })
      .catch((cause: unknown) => {
        if (cancelled) return;
        setError(toFailure(cause));
        setPhase("error");
      });
    return () => {
      cancelled = true;
    };
  }, [payload, send]);

  // 语境轮询：disabled/pending 不改气泡状态，完成或失败都静默处理。
  useEffect(() => {
    if (!senseAvailable || !queryId) return;
    let cancelled = false;
    let attempts = 0;
    let timer: number | undefined;

    const tick = () => {
      attempts += 1;
      send({ type: "lexigrab:sense", queryId })
        .then((raw) => {
          if (cancelled) return;
          const result = raw as SenseResult;
          if (result.status === "resolved") {
            setSense(result.resolution);
            return;
          }
          if (attempts < SENSE_MAX_ATTEMPTS) {
            timer = window.setTimeout(tick, SENSE_INTERVAL_MS);
          }
        })
        .catch(() => {
          // 语境只是建议，失败不打断释义展示。
        });
    };

    timer = window.setTimeout(tick, SENSE_INTERVAL_MS);
    return () => {
      cancelled = true;
      if (timer !== undefined) window.clearTimeout(timer);
    };
  }, [queryId, senseAvailable, send]);

  const save = useCallback(async () => {
    const entry = entries[selected];
    if (!entry || saving) return;
    setSaving(true);
    setSaveError(null);
    const ankiPayload: AnkiPayload = {
      word: snapshot.word,
      definition_html: entry.definition_html,
      sentence: snapshot.sentence,
      source: entry.source_name,
    };
    try {
      const result = (await send({ type: "lexigrab:anki", payload: ankiPayload })) as AnkiResult;
      setSavedNoteId(result.note_id);
      setExactContextSeen(result.exact_context_seen === true);
      setPhase("saved");
    } catch (cause: unknown) {
      setSaveError(toFailure(cause));
    } finally {
      setSaving(false);
    }
  }, [entries, saving, selected, send, snapshot.sentence, snapshot.word]);

  const saveAgain = useCallback(() => {
    setPhase("results");
    setSaveError(null);
    setSavedNoteId(null);
    setExactContextSeen(false);
  }, []);

  const visibleEntries = entries.slice(0, MAX_ENTRIES);
  const currentEntry = entries[selected] ?? visibleEntries[0] ?? null;
  const recommendedConfidence = confidenceText(sense?.confidence ?? null);
  const aiDefinition = sense?.ai ?? null;
  const errorCopy = error ? (ERROR_COPY[error.code] ?? ERROR_COPY.SERVER_ERROR) : null;
  const saveErrorCopy = saveError ? (ERROR_COPY[saveError.code] ?? ERROR_COPY.SERVER_ERROR) : null;

  return (
    <div
      role="dialog"
      aria-label="LexiGrab 查询结果"
      className="lg-interactive absolute flex max-h-[480px] w-[360px] flex-col overflow-hidden rounded-2xl border border-hairline bg-canvas text-ink shadow-2xl"
      style={{ left: position.left, top: position.top }}
    >
      <header className="flex items-start justify-between gap-3 border-b border-hairline px-4 py-3">
        <div className="min-w-0">
          <p className="truncate text-[15px] font-medium" title={snapshot.word}>
            {snapshot.word}
          </p>
          <p className="mt-0.5 flex items-center gap-2 text-[11px] text-ink-muted">
            {currentEntry?.phonetic ? <span className="font-mono">{currentEntry.phonetic}</span> : null}
            <span className="truncate">{snapshot.domain || snapshot.title}</span>
          </p>
        </div>
        <button
          type="button"
          aria-label="关闭"
          onClick={onClose}
          className="flex size-7 shrink-0 items-center justify-center rounded-full text-ink-muted transition-colors hover:bg-canvas-soft hover:text-ink"
        >
          ✕
        </button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        {phase === "loading" ? (
          <p role="status" className="py-6 text-center text-sm text-ink-muted">
            正在查询…
          </p>
        ) : null}

        {phase === "error" && error && errorCopy ? (
          <div role="alert" className="py-2">
            <p className="text-sm font-medium text-danger">{errorCopy.title}</p>
            <p className="mt-1 text-xs text-ink-muted">{errorCopy.action}</p>
            <p className="mt-1 font-mono text-[11px] text-ink-muted">
              {error.code} · {error.message}
            </p>
          </div>
        ) : null}

        {phase === "empty" ? (
          <div className="py-2">
            <p className="text-sm">未找到释义</p>
            <p className="mt-1 text-xs text-ink-muted">
              可以在桌面端添加包含该词的词典，或直接在桌面端手动制卡。
            </p>
          </div>
        ) : null}

        {phase === "results" ? (
          <div className="flex flex-col gap-3">
            {sourceNames.length > 0 ? (
              <p className="text-[11px] text-ink-muted">
                来源：{sourceNames.join(" · ")}
                {partial ? <span className="ml-2">部分结果（有源超时）</span> : null}
              </p>
            ) : null}

            {visibleEntries.length > 1 ? (
              <div className="flex flex-wrap gap-1.5">
                {visibleEntries.map((entry, index) => (
                  <button
                    key={`${entry.source_name}-${entry.headword}-${index}`}
                    type="button"
                    aria-pressed={index === selected}
                    onClick={() => setSelected(index)}
                    className={`rounded-full px-2.5 py-1 text-[11px] transition-colors ${
                      index === selected
                        ? "bg-ink text-on-accent"
                        : "bg-canvas-soft text-ink-muted hover:text-ink"
                    }`}
                  >
                    {entry.source_name || entry.headword}
                  </button>
                ))}
              </div>
            ) : null}

            {currentEntry ? (
              <div
                className="lg-definition"
                // 服务端已清洗，扩展再用 DOMPurify 二次清洗（APP_FLOW §8.2）。
                dangerouslySetInnerHTML={{ __html: sanitize(currentEntry.definition_html) }}
              />
            ) : null}

            {sense && (sense.selected_sense_id || aiDefinition) ? (
              <div className="rounded-xl bg-canvas-soft px-3 py-2">
                {sense.selected_sense_id ? (
                  <p className="text-[11px] text-ink-muted">
                    推荐义项 {sense.selected_sense_id}
                    {recommendedConfidence ? ` · 置信度 ${recommendedConfidence}` : ""}
                    {sense.selector_model ? ` · ${sense.selector_model}` : ""}
                  </p>
                ) : null}
                {aiDefinition ? (
                  <div className="mt-1">
                    <p className="text-[11px] font-medium">
                      AI 生成 · {aiDefinition.model} · 可能不准确
                    </p>
                    <p className="mt-1 text-[13px] leading-relaxed">{aiDefinition.definition_text}</p>
                  </div>
                ) : null}
                <p className="mt-1 text-[10px] text-ink-muted">语境建议仅供参考，保存仍使用所选词典义项。</p>
              </div>
            ) : null}
          </div>
        ) : null}

        {phase === "saved" ? (
          <div className="py-2">
            <p className="text-sm font-medium text-success">已保存</p>
            {savedNoteId !== null ? (
              <p className="mt-1 font-mono text-[11px] text-ink-muted">note_id {savedNoteId}</p>
            ) : null}
            {exactContextSeen ? (
              <p className="mt-2 rounded-lg bg-canvas-soft px-2 py-1 text-[11px] text-ink-muted">
                同词同句同释义此前已保存过；这张卡仍然写入了 Anki。
              </p>
            ) : null}
          </div>
        ) : null}

        {saveError ? (
          <div role="alert" className="mt-3 rounded-xl border border-danger/40 px-3 py-2">
            <p className="text-xs font-medium text-danger">
              {saveErrorCopy?.title ?? "保存失败"}
            </p>
            <p className="mt-0.5 text-[11px] text-ink-muted">{saveErrorCopy?.action}</p>
            <p className="mt-0.5 text-[11px] text-ink-muted">{saveError.message}</p>
            <p className="mt-0.5 font-mono text-[10px] text-ink-muted">{saveError.code}</p>
          </div>
        ) : null}

        {snapshot.sentence ? (
          <p className="mt-3 border-t border-hairline pt-2 text-[11px] leading-relaxed text-ink-muted">
            {snapshot.sentence}
          </p>
        ) : null}

        {sense === null && senseAvailable && phase === "results" ? (
          <p className="mt-2 text-[10px] text-ink-muted">正在获取语境建议…</p>
        ) : null}
      </div>

      <footer className="flex items-center justify-between gap-3 border-t border-hairline px-4 py-3">
        <p className="min-w-0 truncate text-[11px] text-ink-muted">
          {phase === "results" ? plainText(currentEntry?.definition_html ?? "").slice(0, 40) : ""}
        </p>
        {phase === "saved" ? (
          <button
            type="button"
            onClick={saveAgain}
            className="h-8 shrink-0 rounded-full bg-ink px-4 text-xs font-medium text-on-accent"
          >
            再存一张
          </button>
        ) : (
          <button
            type="button"
            onClick={() => void save()}
            disabled={phase !== "results" || saving || currentEntry === null}
            className="h-8 shrink-0 rounded-full bg-ink px-4 text-xs font-medium text-on-accent disabled:opacity-48"
          >
            {saving ? "保存中…" : "保存所选义项"}
          </button>
        )}
      </footer>
    </div>
  );
}

/** 供 content script 复用的锚点类型。 */
export type BubbleAnchor = AnchorRect;
