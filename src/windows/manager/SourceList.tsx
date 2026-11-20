import { useCallback, useEffect, useRef, useState } from "react";
import { onDictIndexProgress } from "../../lib/events";
import {
  getSourceStatuses,
  listSources,
  removeSource,
  toIpcError,
  updateSource,
} from "../../lib/ipc";
import type { IpcError, SourceConfig, SourceStateEntry } from "../../lib/types";
import { Button } from "../../components/Button";
import { EmptyState } from "../../components/EmptyState";
import { Spinner } from "../../components/Spinner";
import { FirstRunGuide, readGuideDismissed, storeGuideDismissed } from "./FirstRunGuide";
import { Modal } from "./Modal";
import { SourceDialog } from "./SourceDialog";
import { SourceStatusBadge } from "./SourceStatus";
import { summarisePaths } from "./paths";

type DialogState = { mode: "add" } | { mode: "edit"; source: SourceConfig };

function sortByPriority(sources: SourceConfig[]): SourceConfig[] {
  return [...sources].sort((left, right) => left.priority - right.priority);
}

/** Row summary: local paths or the remote template + parser, capped for one line. */
function describeSource(source: SourceConfig): { text: string; title: string } {
  if (source.type === "remote_http") {
    const parser = source.response_parser.mode === "jsonpath" ? "JSONPath" : "脚本";
    return {
      text: `远程 API · ${parser} · ${source.url_template}`,
      title: source.url_template,
    };
  }
  return {
    text: source.paths.length > 0 ? summarisePaths(source.paths) : "未选择文件",
    title: source.paths.join("\n"),
  };
}

function IconChevron({ direction }: { direction: "up" | "down" }) {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
      <path
        d={direction === "up" ? "M4 10l4-4 4 4" : "M4 6l4 4 4-4"}
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

interface SourceRowProps {
  source: SourceConfig;
  status: SourceStateEntry | null;
  busy: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onToggle: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRetry: () => void;
}

function SourceRow({
  source,
  status,
  busy,
  canMoveUp,
  canMoveDown,
  onToggle,
  onMoveUp,
  onMoveDown,
  onEdit,
  onDelete,
  onRetry,
}: SourceRowProps) {
  const iconButton =
    "flex size-8 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 " +
    "hover:bg-canvas-soft hover:text-ink disabled:pointer-events-none disabled:opacity-48";

  const summary = describeSource(source);

  return (
    <li className="flex min-h-16 items-center gap-3 border-b border-hairline px-1 py-2 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-sm font-medium" title={source.name}>
            {source.name}
          </p>
          <span className="shrink-0 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
            P{source.priority}
          </span>
          {source.type === "remote_http" ? (
            <span className="shrink-0 rounded-full border border-hairline px-2 py-0.5 font-mono text-[10px] tracking-[0.6px] text-ink-muted">
              远程
            </span>
          ) : null}
        </div>
        <p className="mt-0.5 truncate text-xs text-ink-muted" title={summary.title}>
          {summary.text}
        </p>
      </div>

      <div className="shrink-0">
        <SourceStatusBadge
          enabled={source.enabled}
          status={status?.status ?? null}
          remote={source.type === "remote_http"}
          {...(status?.status.status === "failed" ? { onRetry } : {})}
        />
      </div>

      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={onMoveUp}
          disabled={!canMoveUp || busy}
          aria-label={`上移 ${source.name}`}
          className={iconButton}
        >
          <IconChevron direction="up" />
        </button>
        <button
          type="button"
          onClick={onMoveDown}
          disabled={!canMoveDown || busy}
          aria-label={`下移 ${source.name}`}
          className={iconButton}
        >
          <IconChevron direction="down" />
        </button>
      </div>

      <button
        type="button"
        role="switch"
        aria-checked={source.enabled}
        aria-label={`${source.enabled ? "停用" : "启用"} ${source.name}`}
        onClick={onToggle}
        disabled={busy}
        className={`relative h-5 w-9 shrink-0 rounded-full border transition-colors duration-150 disabled:opacity-48 ${
          source.enabled ? "border-accent bg-accent" : "border-hairline bg-canvas-soft"
        }`}
      >
        <span
          aria-hidden="true"
          className={`absolute top-0.5 size-3.5 rounded-full transition-transform duration-150 ${
            source.enabled ? "left-0.5 bg-on-accent" : "left-4 bg-ink-muted"
          }`}
        />
      </button>

      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={onEdit}
          className="h-8 rounded-full px-2 text-xs text-accent transition-colors duration-150 hover:bg-canvas-soft"
        >
          编辑
        </button>
        <button
          type="button"
          onClick={onDelete}
          className="h-8 rounded-full px-2 text-xs text-danger transition-colors duration-150 hover:bg-canvas-soft"
        >
          删除
        </button>
      </div>
    </li>
  );
}

interface SourceListProps {
  /** `?guide=1` reopens the first-run guide even after dismissal. */
  forceGuide: boolean;
  /** Clears the forced-guide state after the guide is dismissed. */
  onGuideClosed: () => void;
}

/**
 * Source management list (docs/APP_FLOW.md §5): rows, status, enable, sort,
 * edit/delete and the first-run guide. Status reloads after mutations because
 * opening a dictionary happens asynchronously in the backend.
 */
export function SourceList({ forceGuide, onGuideClosed }: SourceListProps) {
  const [sources, setSources] = useState<SourceConfig[] | null>(null);
  const [statuses, setStatuses] = useState<SourceStateEntry[] | null>(null);
  const [listError, setListError] = useState<IpcError | null>(null);
  const [statusError, setStatusError] = useState<IpcError | null>(null);
  const [actionError, setActionError] = useState<IpcError | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyIds, setBusyIds] = useState<string[]>([]);
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<SourceConfig | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<IpcError | null>(null);
  const [dismissed, setDismissed] = useState(readGuideDismissed);
  const [nonce, setNonce] = useState(0);
  const pollCount = useRef(0);

  useEffect(() => {
    let active = true;
    Promise.allSettled([listSources(), getSourceStatuses()])
      .then(([sourcesResult, statusesResult]) => {
        if (!active) return;
        if (sourcesResult.status === "fulfilled") {
          setSources(sortByPriority(sourcesResult.value));
          setListError(null);
        } else {
          setListError(toIpcError(sourcesResult.reason));
        }
        if (statusesResult.status === "fulfilled") {
          setStatuses(statusesResult.value);
          setStatusError(null);
        } else {
          setStatusError(toIpcError(statusesResult.reason));
        }
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  /** Reloads list state in the background without hiding current rows. */
  const reloadSilently = useCallback(() => setNonce((value) => value + 1), []);

  /** Reloads with a visible loading state, used by explicit retry actions. */
  const reloadWithSpinner = useCallback(() => {
    setLoading(true);
    setListError(null);
    setStatusError(null);
    setNonce((value) => value + 1);
  }, []);

  const refreshStatuses = useCallback(async () => {
    try {
      const next = await getSourceStatuses();
      setStatuses(next);
      setStatusError(null);
    } catch (cause: unknown) {
      setStatusError(toIpcError(cause));
    }
  }, []);

  // `dict://index-progress` fires while dictionaries open; coalesce bursts.
  useEffect(() => {
    let stop: (() => void) | null = null;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | null = null;
    onDictIndexProgress(() => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        void refreshStatuses();
      }, 250);
    })
      .then((unlisten) => {
        if (active) stop = unlisten;
        else unlisten();
      })
      .catch(() => {
        // Event subscription failure still leaves the polling fallback below.
      });
    return () => {
      active = false;
      if (timer) clearTimeout(timer);
      stop?.();
    };
  }, [refreshStatuses]);

  // Poll while any enabled source is still loading or not yet registered.
  useEffect(() => {
    if (!sources || !statuses) return;
    const missing = sources.some(
      (source) =>
        source.enabled &&
        !statuses.some((entry) => entry.id === source.id && entry.status.status !== "loading"),
    );
    const anyLoading = statuses.some((entry) => entry.status.status === "loading");
    if (!missing && !anyLoading) {
      pollCount.current = 0;
      return;
    }
    if (pollCount.current >= 20) return;
    const timer = setTimeout(() => {
      pollCount.current += 1;
      void refreshStatuses();
    }, 1000);
    return () => clearTimeout(timer);
  }, [sources, statuses, refreshStatuses]);

  const markBusy = (id: string, busy: boolean) => {
    setBusyIds((current) =>
      busy ? [...current, id] : current.filter((item) => item !== id),
    );
  };

  const toggleEnabled = async (source: SourceConfig) => {
    if (busyIds.includes(source.id)) return;
    setActionError(null);
    markBusy(source.id, true);
    setSources((current) =>
      current?.map((item) => (item.id === source.id ? { ...item, enabled: !item.enabled } : item)) ??
      current,
    );
    try {
      const saved = await updateSource(source.id, { ...source, enabled: !source.enabled });
      setSources((current) =>
        current?.map((item) => (item.id === source.id ? saved : item)) ?? current,
      );
      pollCount.current = 0;
      void refreshStatuses();
    } catch (cause: unknown) {
      setActionError(toIpcError(cause));
      setSources((current) =>
        current?.map((item) => (item.id === source.id ? source : item)) ?? current,
      );
    } finally {
      markBusy(source.id, false);
    }
  };

  const move = async (source: SourceConfig, direction: -1 | 1) => {
    if (!sources || busyIds.includes(source.id)) return;
    const ordered = sortByPriority(sources);
    const index = ordered.findIndex((item) => item.id === source.id);
    const targetIndex = index + direction;
    if (index < 0 || targetIndex < 0 || targetIndex >= ordered.length) return;
    const swapped = [...ordered];
    const neighbour = ordered[targetIndex];
    if (!neighbour) return;
    swapped[index] = neighbour;
    swapped[targetIndex] = source;
    const next = swapped.map((item, position) => ({ ...item, priority: (position + 1) * 10 }));
    const previousById = new Map(ordered.map((item) => [item.id, item.priority]));

    setActionError(null);
    setSources(next);
    markBusy(source.id, true);
    try {
      for (const item of next) {
        if (previousById.get(item.id) !== item.priority) {
          await updateSource(item.id, item);
        }
      }
      void refreshStatuses();
    } catch (cause: unknown) {
      setActionError(toIpcError(cause));
      reloadSilently();
    } finally {
      markBusy(source.id, false);
    }
  };

  const retrySource = async (source: SourceConfig) => {
    if (busyIds.includes(source.id)) return;
    setActionError(null);
    markBusy(source.id, true);
    try {
      await updateSource(source.id, source);
      pollCount.current = 0;
      void refreshStatuses();
    } catch (cause: unknown) {
      setActionError(toIpcError(cause));
    } finally {
      markBusy(source.id, false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    setDeleteError(null);
    try {
      await removeSource(deleteTarget.id);
      setDeleteTarget(null);
      reloadSilently();
    } catch (cause: unknown) {
      setDeleteError(toIpcError(cause));
    } finally {
      setDeleting(false);
    }
  };

  const handleSaved = () => {
    setDialog(null);
    pollCount.current = 0;
    reloadSilently();
    void refreshStatuses();
  };

  const dismissGuide = useCallback(() => {
    storeGuideDismissed();
    setDismissed(true);
    onGuideClosed();
  }, [onGuideClosed]);

  const ordered = sources ? sortByPriority(sources) : [];
  const showGuide = (forceGuide || !dismissed) && !loading && listError === null;
  const addDialog = (
    <SourceDialog
      mode="add"
      source={null}
      existingSources={ordered}
      onClose={() => setDialog(null)}
      onSaved={handleSaved}
    />
  );

  return (
    <section aria-labelledby="sources-title" className="flex flex-col gap-6">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 id="sources-title" className="text-[26px] font-medium tracking-tight">
            词典源
          </h1>
          <p className="mt-1 text-sm text-ink-muted">
            管理本地 .mdx 词典与远程 API 源；列表按优先级升序，启停与排序会立即保存。
          </p>
        </div>
        <Button onClick={() => setDialog({ mode: "add" })}>添加源</Button>
      </header>

      {actionError ? (
        <div role="alert" className="flex items-start justify-between gap-4 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
          <div>
            <p className="font-medium text-danger">操作失败</p>
            <p className="mt-1 text-ink-muted">{actionError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {actionError.code}
            </p>
          </div>
          <button
            type="button"
            onClick={() => setActionError(null)}
            className="shrink-0 rounded-full px-3 py-1 text-xs text-ink-muted transition-colors duration-150 hover:bg-canvas-soft"
          >
            关闭
          </button>
        </div>
      ) : null}

      {showGuide ? (
        <FirstRunGuide
          sourceCount={sources?.length ?? 0}
          force={forceGuide}
          onAddSource={() => setDialog({ mode: "add" })}
          onDismiss={dismissGuide}
        />
      ) : null}

      <div>
        {loading ? (
          <div className="flex items-center gap-2 border-y border-hairline px-1 py-8">
            <Spinner label="正在加载词典源…" />
          </div>
        ) : listError ? (
          <div role="alert" className="rounded-3xl border border-danger/40 px-6 py-8">
            <p className="text-sm font-medium text-danger">无法读取词典源</p>
            <p className="mt-1 text-sm text-ink-muted">{listError.message}</p>
            <Button
              variant="secondary"
              size="sm"
              className="mt-3"
              onClick={reloadWithSpinner}
            >
              重试
            </Button>
          </div>
        ) : ordered.length === 0 ? (
          <EmptyState
            tone={showGuide ? "plain" : "block"}
            title="还没有数据源"
            description="添加本地 .mdx 词典或远程 API 源后即可查询。本地词典只读取文件，不会修改或删除原文件。"
            action={<Button onClick={() => setDialog({ mode: "add" })}>添加数据源</Button>}
          />
        ) : (
          <div>
            <div className="flex items-center justify-between border-b border-hairline pb-2">
              <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">源列表</span>
              <span className="font-mono text-[11px] text-ink-muted">{ordered.length} 项</span>
            </div>
            {statusError ? (
              <div role="alert" className="flex items-center justify-between gap-4 border-b border-hairline py-2 text-xs">
                <span className="text-danger">无法读取词典状态：{statusError.message}</span>
                <button
                  type="button"
                  onClick={() => void refreshStatuses()}
                  className="rounded-full px-2 py-1 text-accent transition-colors duration-150 hover:bg-canvas-soft"
                >
                  重试
                </button>
              </div>
            ) : null}
            <ul aria-label="词典源列表">
              {ordered.map((source, index) => (
                <SourceRow
                  key={source.id}
                  source={source}
                  status={statuses?.find((entry) => entry.id === source.id) ?? null}
                  busy={busyIds.includes(source.id)}
                  canMoveUp={index > 0}
                  canMoveDown={index < ordered.length - 1}
                  onToggle={() => void toggleEnabled(source)}
                  onMoveUp={() => void move(source, -1)}
                  onMoveDown={() => void move(source, 1)}
                  onEdit={() => setDialog({ mode: "edit", source })}
                  onDelete={() => {
                    setDeleteError(null);
                    setDeleteTarget(source);
                  }}
                  onRetry={() => void retrySource(source)}
                />
              ))}
            </ul>
          </div>
        )}
      </div>

      {dialog?.mode === "add" ? addDialog : null}
      {dialog?.mode === "edit" ? (
        <SourceDialog
          mode="edit"
          source={dialog.source}
          existingSources={ordered}
          onClose={() => setDialog(null)}
          onSaved={handleSaved}
        />
      ) : null}

      {deleteTarget ? (
        <Modal labelledBy="delete-source-title" onClose={() => setDeleteTarget(null)}>
          <h2 id="delete-source-title" className="text-[16px] font-medium">
            删除数据源？
          </h2>
          <p className="mt-2 text-sm text-ink-muted">
            {deleteTarget.type === "remote_http"
              ? `将移除远程源「${deleteTarget.name}」的配置，不会影响该服务端。`
              : `将移除「${deleteTarget.name}」的配置。磁盘上的词典文件不会被删除。`}
          </p>
          {deleteError ? (
            <p role="alert" className="mt-3 text-sm text-danger">
              删除失败：{deleteError.message}
            </p>
          ) : null}
          <div className="mt-6 flex items-center justify-end gap-3 border-t border-hairline pt-4">
            <Button variant="secondary" onClick={() => setDeleteTarget(null)} disabled={deleting}>
              取消
            </Button>
            <Button
              variant="ghost"
              onClick={() => void confirmDelete()}
              disabled={deleting}
              className="text-danger hover:bg-canvas-soft"
            >
              {deleting ? "删除中…" : "删除配置文件"}
            </Button>
          </div>
        </Modal>
      ) : null}
    </section>
  );
}
