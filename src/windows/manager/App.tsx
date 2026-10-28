import { useCallback, useEffect, useState } from "react";
import { getRuntimeStatus } from "../../lib/ipc";
import type { RuntimeStatus } from "../../lib/types";
import { CaptureTestPanel } from "./CaptureTestPanel";
import { SourcesPanel } from "./SourcesPanel";

const VIEWS = [
  { id: "sources", label: "词典源" },
  { id: "test", label: "查询测试" },
  { id: "status", label: "运行状态" },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

function readView(): ViewId {
  const raw = new URLSearchParams(window.location.search).get("view");
  return VIEWS.find((view) => view.id === raw)?.id ?? "sources";
}

function navigate(view: ViewId): void {
  const url = new URL(window.location.href);
  url.searchParams.set("view", view);
  window.history.replaceState(null, "", url);
}

function useRuntimeStatus(): {
  status: RuntimeStatus | null;
  error: string | null;
  loading: boolean;
  refresh: () => void;
} {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    getRuntimeStatus()
      .then((value) => {
        if (!active) return;
        setStatus(value);
        setError(null);
      })
      .catch((cause: unknown) => {
        if (!active) return;
        const message = cause instanceof Error ? cause.message : "无法读取运行状态";
        setError(message);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);
  return { status, error, loading, refresh };
}

function StatusBadge({ ok, label }: { ok: boolean; label: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-xs text-ink-muted">
      <span
        aria-hidden="true"
        className={`size-1.5 rounded-full ${ok ? "bg-success" : "bg-danger"}`}
      />
      <span className="font-mono tracking-wide">{label}</span>
    </span>
  );
}

function StatusPanel({
  runtime,
}: {
  runtime: ReturnType<typeof useRuntimeStatus>;
}) {
  const { status, error, loading, refresh } = runtime;
  return (
    <section aria-labelledby="status-title" className="flex max-w-[760px] flex-col gap-8">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 id="status-title" className="text-[26px] font-medium tracking-tight">
            运行状态
          </h1>
          <p className="mt-1 text-sm text-ink-muted">热键、版本与本地目录。M0 阶段只展示验证信息。</p>
        </div>
        <button
          type="button"
          onClick={refresh}
          className="h-10 shrink-0 rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft active:bg-canvas-soft"
        >
          刷新
        </button>
      </header>

      {loading ? (
        <div role="status" className="rounded-3xl border border-hairline p-6 text-sm text-ink-muted">
          正在读取运行状态…
        </div>
      ) : error ? (
        <div role="alert" className="rounded-3xl border border-danger/40 p-6 text-sm">
          <p className="font-medium text-danger">读取运行状态失败</p>
          <p className="mt-1 text-ink-muted">{error}</p>
        </div>
      ) : status ? (
        <dl className="divide-y divide-hairline rounded-3xl border border-hairline">
          <div className="flex items-center justify-between gap-6 px-6 py-4">
            <dt className="text-sm text-ink-muted">版本</dt>
            <dd className="font-mono text-xs">{status.version}</dd>
          </div>
          <div className="flex items-center justify-between gap-6 px-6 py-4">
            <dt className="text-sm text-ink-muted">全局热键</dt>
            <dd className="flex items-center gap-3">
              <span className="rounded-full border border-hairline px-2 py-1 font-mono text-xs">
                {status.hotkey.accelerator}
              </span>
              <StatusBadge
                ok={status.hotkey.registered}
                label={status.hotkey.registered ? "已注册" : "未注册"}
              />
            </dd>
          </div>
          {status.hotkey.message ? (
            <div className="px-6 py-4">
              <p className="text-sm text-danger">{status.hotkey.message}</p>
            </div>
          ) : null}
          <div className="flex items-center justify-between gap-6 px-6 py-4">
            <dt className="text-sm text-ink-muted">配置目录</dt>
            <dd className="max-w-[420px] truncate font-mono text-xs" title={status.config_dir}>
              {status.config_dir}
            </dd>
          </div>
          <div className="flex items-center justify-between gap-6 px-6 py-4">
            <dt className="text-sm text-ink-muted">日志目录</dt>
            <dd className="max-w-[420px] truncate font-mono text-xs" title={status.log_dir}>
              {status.log_dir}
            </dd>
          </div>
        </dl>
      ) : null}

      <CaptureTestPanel />
    </section>
  );
}

export function App() {
  const [view, setView] = useState<ViewId>(readView);
  const runtime = useRuntimeStatus();

  const select = (next: ViewId) => {
    navigate(next);
    setView(next);
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-[220px] shrink-0 flex-col border-r border-hairline bg-canvas-soft px-4 py-6">
        <div className="px-2">
          <p className="text-[16px] font-medium tracking-tight">LexiGrab</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">M0 技术验证</p>
        </div>
        <nav aria-label="主导航" className="mt-8 flex flex-col gap-1">
          {VIEWS.map((item) => {
            const active = item.id === view;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => select(item.id)}
                className={`flex h-11 items-center rounded-xl px-3 text-left text-sm transition-colors duration-150 ${
                  active
                    ? "bg-canvas font-medium text-ink ring-1 ring-hairline"
                    : "text-ink-muted hover:bg-canvas hover:text-ink"
                }`}
              >
                {item.label}
              </button>
            );
          })}
        </nav>
        <p className="mt-auto px-2 font-mono text-[11px] leading-5 text-ink-muted">
          0.1.0 · Windows
        </p>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto px-10 py-8">
        {view === "sources" ? <SourcesPanel /> : null}
        {view === "test" ? (
          <section aria-labelledby="test-title" className="max-w-[760px]">
            <h1 id="test-title" className="text-[26px] font-medium tracking-tight">
              查询测试
            </h1>
            <p className="mt-1 text-sm text-ink-muted">M1 接入词典源后开放。</p>
            <div className="mt-8 rounded-3xl border border-dashed border-hairline px-6 py-12 text-center text-sm text-ink-muted">
              还没有可查询的词典源。
            </div>
          </section>
        ) : null}
        {view === "status" ? <StatusPanel runtime={runtime} /> : null}
      </main>
    </div>
  );
}
