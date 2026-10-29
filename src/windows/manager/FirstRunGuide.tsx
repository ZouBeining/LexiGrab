import { useCallback, useEffect, useState } from "react";
import { getRuntimeStatus, getSettings, openWindow } from "../../lib/ipc";
import type { RuntimeStatus, Settings } from "../../lib/types";

const GUIDE_DISMISSED_KEY = "lexigrab.guide.dismissed";

/** Reads the persisted "guide dismissed" flag; storage failure keeps the guide visible. */
export function readGuideDismissed(): boolean {
  try {
    return window.localStorage.getItem(GUIDE_DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

export function storeGuideDismissed(): void {
  try {
    window.localStorage.setItem(GUIDE_DISMISSED_KEY, "1");
  } catch {
    // Dismissal is a convenience; persistence failure must not break the window.
  }
}

export function clearGuideDismissed(): void {
  try {
    window.localStorage.removeItem(GUIDE_DISMISSED_KEY);
  } catch {
    // Same as above.
  }
}

interface GuideStep {
  key: string;
  title: string;
  description: string;
  done: boolean;
  actionLabel: string | null;
  onAction: (() => void) | null;
}

interface FirstRunGuideProps {
  sourceCount: number;
  /** `?guide=1` forces the guide open even after it was dismissed. */
  force: boolean;
  onAddSource: () => void;
  onDismiss: () => void;
}

function CheckGlyph({ done }: { done: boolean }) {
  if (done) {
    return (
      <span
        aria-hidden="true"
        className="flex size-6 shrink-0 items-center justify-center rounded-full bg-success text-canvas"
      >
        <svg viewBox="0 0 16 16" className="size-3.5" fill="none">
          <path d="M3.5 8.5l3 3 6-7" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
        </svg>
      </span>
    );
  }
  return (
    <span
      aria-hidden="true"
      className="flex size-6 shrink-0 items-center justify-center rounded-full border border-hairline bg-canvas"
    />
  );
}

/**
 * First-run guide from docs/APP_FLOW.md §4: three steps with live completion marks.
 * It only reads existing commands; no new IPC surface is introduced.
 */
export function FirstRunGuide({ sourceCount, force, onAddSource, onDismiss }: FirstRunGuideProps) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    Promise.allSettled([getSettings(), getRuntimeStatus()])
      .then(([settingsResult, runtimeResult]) => {
        if (!active) return;
        if (settingsResult.status === "fulfilled") {
          setSettings(settingsResult.value);
        } else {
          setError("无法读取设置，引导中的完成状态可能不准确。");
        }
        if (runtimeResult.status === "fulfilled") {
          setRuntime(runtimeResult.value);
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  const retry = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);

  const openSettings = useCallback(() => {
    void openWindow("settings");
  }, []);

  const ankiConfigured =
    settings !== null &&
    settings.anki.endpoint.trim().length > 0 &&
    settings.anki.deck.trim().length > 0 &&
    settings.anki.model.trim().length > 0;
  const hotkeyRegistered = runtime?.hotkey.registered === true;

  const steps: GuideStep[] = [
    {
      key: "source",
      title: "添加词典",
      description: "选择本地 .mdx 词典文件，同名 .mdd 会自动配对。",
      done: sourceCount > 0,
      actionLabel: sourceCount > 0 ? null : "添加词典",
      onAction: sourceCount > 0 ? null : onAddSource,
    },
    {
      key: "anki",
      title: "配置 Anki",
      description: "填写 AnkiConnect 地址、牌组和笔记类型后即可制卡。",
      done: ankiConfigured,
      actionLabel: ankiConfigured ? null : "打开设置",
      onAction: ankiConfigured ? null : openSettings,
    },
    {
      key: "hotkey",
      title: "设置热键",
      description: "确认全局热键已注册，选中文字后按下即可呼出弹窗。",
      done: hotkeyRegistered,
      actionLabel: hotkeyRegistered ? null : "检查热键",
      onAction: hotkeyRegistered ? null : openSettings,
    },
  ];

  const allDone = steps.every((step) => step.done);

  useEffect(() => {
    if (!loading && allDone && !force) onDismiss();
  }, [allDone, force, loading, onDismiss]);

  if (allDone && !force) return null;

  return (
    <section aria-labelledby="guide-title" className="rounded-3xl bg-block-3 p-6">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 id="guide-title" className="text-[16px] font-medium">
            三步完成初始配置
          </h2>
          <p className="mt-1 text-sm text-ink-muted">
            每完成一步会自动打勾。也可以先跳过，之后在运行状态里查看。
          </p>
        </div>
        <button
          type="button"
          onClick={onDismiss}
          aria-label="关闭首次引导"
          className="flex size-8 shrink-0 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 hover:bg-canvas hover:text-ink"
        >
          <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
            <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </button>
      </div>

      {error ? (
        <div role="alert" className="mt-4 flex items-center justify-between gap-4 rounded-2xl border border-danger/40 bg-canvas px-4 py-3 text-sm">
          <span className="text-danger">{error}</span>
          <button
            type="button"
            onClick={retry}
            className="shrink-0 rounded-full border border-hairline px-3 py-1 text-xs transition-colors duration-150 hover:bg-canvas-soft"
          >
            重试
          </button>
        </div>
      ) : null}

      <ol className="mt-5 flex flex-col gap-4">
        {steps.map((step, index) => (
          <li key={step.key} className="flex items-start gap-3">
            <CheckGlyph done={step.done} />
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 text-sm font-medium">
                <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">0{index + 1}</span>
                <span>{step.title}</span>
                <span className="text-xs font-normal text-ink-muted">
                  {loading && step.key !== "source" ? "检查中…" : step.done ? "已完成" : "未完成"}
                </span>
              </p>
              <p className="mt-1 text-sm text-ink-muted">{step.description}</p>
            </div>
            {step.actionLabel && step.onAction ? (
              <button
                type="button"
                onClick={step.onAction}
                className="h-8 shrink-0 rounded-full border border-hairline bg-canvas px-3 text-xs transition-colors duration-150 hover:bg-canvas-soft"
              >
                {step.actionLabel}
              </button>
            ) : null}
          </li>
        ))}
      </ol>
    </section>
  );
}
