import type { SourceStatus } from "../../lib/types";

interface SourceStatusBadgeProps {
  /** Whether the source is enabled in config; disabled sources never open a worker. */
  enabled: boolean;
  /** Runtime status when the source was returned by `get_source_statuses`. */
  status: SourceStatus | null;
  /** Remote sources have no index; show “ready / last failed” instead of entry counts. */
  remote?: boolean;
  onRetry?: () => void;
}

function Dot({ tone }: { tone: "muted" | "success" | "danger" | "loading" }) {
  if (tone === "loading") {
    return (
      <span
        aria-hidden="true"
        className="size-3 shrink-0 animate-spin rounded-full border border-hairline border-t-accent"
      />
    );
  }
  const color =
    tone === "success" ? "bg-success" : tone === "danger" ? "bg-danger" : "bg-ink-muted";
  return <span aria-hidden="true" className={`size-1.5 shrink-0 rounded-full ${color}`} />;
}

/**
 * Source status badge: text plus a small dot so state never depends on color alone.
 * States follow docs/APP_FLOW.md §5.1 and PLAN_M4 T4.6:
 * disabled / loading / ready / failed; remote rows omit entry counts.
 */
export function SourceStatusBadge({ enabled, status, remote = false, onRetry }: SourceStatusBadgeProps) {
  if (!enabled) {
    return (
      <span className="inline-flex items-center gap-2 text-xs text-ink-muted">
        <Dot tone="muted" />
        <span>已停用</span>
      </span>
    );
  }

  if (status === null || status.status === "loading") {
    return (
      <span className="inline-flex items-center gap-2 text-xs text-ink-muted" role="status">
        <Dot tone="loading" />
        <span>{remote ? "正在连接" : "正在加载"}</span>
      </span>
    );
  }

  if (status.status === "ready") {
    return (
      <span className="inline-flex items-center gap-2 text-xs text-ink-muted">
        <Dot tone="success" />
        <span>{remote ? "就绪" : `${status.entry_count} 条目`}</span>
      </span>
    );
  }

  return (
    <span className="inline-flex min-w-0 items-center gap-2 text-xs text-danger">
      <Dot tone="danger" />
      <span className="max-w-[180px] truncate" title={status.message}>
        {remote ? "上次失败：" : "加载失败："}
        {status.message}
      </span>
      {onRetry ? (
        <button
          type="button"
          onClick={onRetry}
          className="shrink-0 rounded-full px-2 py-0.5 text-xs text-accent transition-colors duration-150 hover:bg-canvas-soft"
        >
          重试
        </button>
      ) : null}
    </span>
  );
}
