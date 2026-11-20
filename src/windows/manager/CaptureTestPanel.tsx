// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";
import { debugTriggerCapture, getCaptureStats, toIpcError } from "../../lib/ipc";
import type { CaptureResult, CaptureStats, IpcError } from "../../lib/types";

function formatMs(value: number | null): string {
  return value === null ? "—" : `${Math.round(value)} ms`;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-1 rounded-2xl border border-hairline px-4 py-3">
      <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">{label}</span>
      <span className="text-sm font-medium">{value}</span>
    </div>
  );
}

export function CaptureTestPanel() {
  const [result, setResult] = useState<CaptureResult | null>(null);
  const [stats, setStats] = useState<CaptureStats | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [busy, setBusy] = useState(false);

  const loadStats = useCallback(() => {
    getCaptureStats()
      .then(setStats)
      .catch((cause: unknown) => setError(toIpcError(cause)));
  }, []);

  useEffect(() => {
    loadStats();
  }, [loadStats]);

  const capture = useCallback(() => {
    setBusy(true);
    debugTriggerCapture()
      .then((value) => {
        setResult(value);
        setError(value.error);
        loadStats();
      })
      .catch((cause: unknown) => setError(toIpcError(cause)))
      .finally(() => setBusy(false));
  }, [loadStats]);

  return (
    <section aria-labelledby="capture-title" className="flex flex-col gap-4">
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 id="capture-title" className="text-[16px] font-medium">
            捕获测试
          </h2>
          <p className="mt-1 max-w-[460px] text-sm text-ink-muted">
            优先用全局热键在目标应用里触发；此处按钮用于查看最近一次结果并重新捕获。
          </p>
        </div>
        <button
          type="button"
          onClick={capture}
          disabled={busy}
          className="h-10 shrink-0 rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft disabled:opacity-50"
        >
          {busy ? "捕获中…" : "重新捕获"}
        </button>
      </div>

      {error ? (
        <div role="alert" className="rounded-2xl border border-danger/40 px-5 py-4 text-sm">
          <p className="font-medium text-danger">{error.message}</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{error.code}</p>
        </div>
      ) : result?.selection ? (
        <div className="rounded-2xl border border-hairline px-5 py-4">
          <p className="text-sm leading-6 break-words">{result.selection.text}</p>
          <dl className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
            <Metric label="方法" value={result.selection.method} />
            <Metric label="应用" value={result.selection.app_name} />
            <Metric label="耗时" value={formatMs(result.selection.elapsed_ms)} />
            <Metric
              label="锚点"
              value={
                result.selection.anchor
                  ? `${Math.round(result.selection.anchor.x)}, ${Math.round(result.selection.anchor.y)}`
                  : "无"
              }
            />
          </dl>
        </div>
      ) : (
        <div className="rounded-2xl border border-dashed border-hairline px-5 py-8 text-center text-sm text-ink-muted">
          还没有捕获记录。在任意应用选中文字后按热键。
        </div>
      )}

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Metric label="样本" value={stats ? `${stats.samples.length}` : "—"} />
        <Metric label="P50" value={formatMs(stats?.p50_ms ?? null)} />
        <Metric label="P95" value={formatMs(stats?.p95_ms ?? null)} />
        <Metric
          label="成功率"
          value={stats?.success_rate === null || stats === null ? "—" : `${Math.round(stats.success_rate * 100)}%`}
        />
      </div>
    </section>
  );
}
