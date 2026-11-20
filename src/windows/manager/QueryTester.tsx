// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useMemo, useState } from "react";
import { listSources, testSourceQuery, toIpcError } from "../../lib/ipc";
import type { IpcError, SourceConfig, TestResult } from "../../lib/types";
import { Button } from "../../components/Button";
import { EmptyState } from "../../components/EmptyState";
import { Spinner } from "../../components/Spinner";
import { TestResultView } from "./TestResultView";

interface SourceQueryResult {
  source: SourceConfig;
  result: TestResult | null;
  error: IpcError | null;
}

interface FailureInfo {
  code: string;
  message: string;
}

/** A remote source reports failures in `result.error`; local failures reject. */
function groupFailure(group: SourceQueryResult): FailureInfo | null {
  if (group.error) return group.error;
  return group.result?.error ?? null;
}

function suggestionFor(code: string): string {
  switch (code) {
    case "DICT_NOT_READY":
      return "词典仍在加载，稍后重试；仍失败时打开词典源页查看状态。";
    case "DICT_OPEN_FAILED":
      return "词典文件无法打开，检查文件是否被移动或损坏，必要时重新添加。";
    case "DICT_BUSY":
      return "查询队列繁忙，等待片刻后重试。";
    case "SOURCE_NOT_FOUND":
      return "源配置已不存在，回到词典源页刷新列表。";
    case "QUERY_EMPTY":
      return "请输入要查询的词。";
    default:
      if (code.startsWith("REMOTE") || code.startsWith("SCRIPT")) {
        return "检查 URL 模板、鉴权头与解析配置；原始响应预览可帮助定位格式问题。";
      }
      return "稍后重试；如果持续失败，打开运行状态页查看日志目录。";
  }
}

/**
 * Query tester (docs/APP_FLOW.md §5.4, PLAN_M4 T4.6). Uses draft source configs
 * through `test_source_query`: no cache writes, no popup, no Anki export.
 * Remote groups show elapsed time, readable errors and a raw response preview.
 */
export function QueryTester() {
  const [sources, setSources] = useState<SourceConfig[] | null>(null);
  const [sourcesError, setSourcesError] = useState<IpcError | null>(null);
  const [reloadNonce, setReloadNonce] = useState(0);
  const [selection, setSelection] = useState("all");
  const [query, setQuery] = useState("");
  const [queryError, setQueryError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [groups, setGroups] = useState<SourceQueryResult[] | null>(null);
  const [runError, setRunError] = useState<IpcError | null>(null);

  useEffect(() => {
    let active = true;
    listSources()
      .then((value) => {
        if (active) setSources(value);
      })
      .catch((cause: unknown) => {
        if (active) setSourcesError(toIpcError(cause));
      });
    return () => {
      active = false;
    };
  }, [reloadNonce]);

  const reloadSources = () => {
    setSources(null);
    setSourcesError(null);
    setReloadNonce((value) => value + 1);
  };

  const enabledSources = useMemo(
    () => (sources ?? []).filter((source) => source.enabled),
    [sources],
  );

  const selectedSources = useMemo(() => {
    if (selection === "all") return enabledSources;
    const found = enabledSources.find((source) => source.id === selection);
    return found ? [found] : [];
  }, [enabledSources, selection]);

  const run = async () => {
    const trimmed = query.trim();
    if (trimmed.length === 0) {
      setQueryError("请输入要查询的词。");
      return;
    }
    if (selectedSources.length === 0) {
      setQueryError("没有可查询的源，请先启用或添加词典。");
      return;
    }
    setQueryError(null);
    setRunError(null);
    setRunning(true);
    setGroups(null);
    try {
      const settled = await Promise.all(
        selectedSources.map(async (source): Promise<SourceQueryResult> => {
          try {
            const result = await testSourceQuery(source, trimmed);
            return { source, result, error: null };
          } catch (cause: unknown) {
            return { source, result: null, error: toIpcError(cause) };
          }
        }),
      );
      setGroups(settled);
    } catch (cause: unknown) {
      setRunError(toIpcError(cause));
    } finally {
      setRunning(false);
    }
  };

  const clear = () => {
    setQuery("");
    setQueryError(null);
    setRunError(null);
    setGroups(null);
  };

  const entries = groups
    ? groups.flatMap((group) => (group.result ? group.result.entries : []))
    : [];
  const failedGroups = groups?.filter((group) => groupFailure(group) !== null) ?? [];
  const groupsWithOutput =
    groups?.filter(
      (group) =>
        (group.result?.entries.length ?? 0) > 0 ||
        group.result?.raw_preview !== null ||
        groupFailure(group) !== null,
    ) ?? [];
  const maxElapsed =
    groups && groups.length > 0
      ? Math.max(...groups.map((group) => group.result?.elapsed_ms ?? 0))
      : 0;

  const sourcesSection = (() => {
    if (sources === null && sourcesError === null) {
      return (
        <div className="flex items-center gap-2">
          <Spinner label="正在读取词典源…" />
        </div>
      );
    }
    if (sourcesError) {
      return (
        <div role="alert" className="flex items-center justify-between gap-4 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
          <span className="text-danger">无法读取词典源：{sourcesError.message}</span>
          <Button variant="secondary" size="sm" onClick={reloadSources}>
            重试
          </Button>
        </div>
      );
    }
    if (enabledSources.length === 0) {
      return (
        <EmptyState
          title="没有启用的数据源"
          description="先在「词典源」页添加或启用本地词典或远程 API，然后回到这里测试查询。"
        />
      );
    }
    return null;
  })();

  return (
    <section aria-labelledby="tester-title" className="flex flex-col gap-6">
      <header>
        <h1 id="tester-title" className="text-[26px] font-medium tracking-tight">
          查询测试
        </h1>
        <p className="mt-1 text-sm text-ink-muted">
          使用当前配置做一次草稿查询：不写缓存，不弹出划词窗口，也不会写入 Anki。
        </p>
      </header>

      {sourcesSection}

      {sourcesError === null && sources !== null && enabledSources.length > 0 ? (
        <form
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            void run();
          }}
          className="flex flex-col gap-4 rounded-3xl border border-hairline px-6 py-5"
        >
          <div className="flex flex-col gap-2">
            <label htmlFor="tester-source" className="text-sm font-medium">
              查询范围
            </label>
            <select
              id="tester-source"
              value={selection}
              onChange={(event) => setSelection(event.target.value)}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            >
              <option value="all">全部启用源（{enabledSources.length}）</option>
              {enabledSources.map((source) => (
                <option key={source.id} value={source.id}>
                  {source.name}
                  {source.type === "remote_http" ? "（远程）" : ""}
                </option>
              ))}
            </select>
          </div>

          <div className="flex flex-col gap-2">
            <label htmlFor="tester-query" className="text-sm font-medium">
              要查询的词
            </label>
            <div className="flex gap-2">
              <input
                id="tester-query"
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  setQueryError(null);
                }}
                aria-invalid={queryError ? true : undefined}
                aria-describedby={queryError ? "tester-query-error" : undefined}
                placeholder="例如：serendipity"
                className="h-10 min-w-0 flex-1 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
              <Button type="submit" disabled={running}>
                {running ? "查询中…" : "查询"}
              </Button>
              {groups || query.length > 0 ? (
                <Button type="button" variant="secondary" onClick={clear} disabled={running}>
                  清空
                </Button>
              ) : null}
            </div>
            {queryError ? (
              <p id="tester-query-error" role="alert" className="text-xs text-danger">
                {queryError}
              </p>
            ) : null}
          </div>
        </form>
      ) : null}

      {runError ? (
        <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
          <p className="font-medium text-danger">查询失败</p>
          <p className="mt-1 text-ink-muted">{runError.message}</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{runError.code}</p>
          <p className="mt-2 text-xs text-ink-muted">{suggestionFor(runError.code)}</p>
        </div>
      ) : null}

      {running ? (
        <div className="flex items-center gap-2 border-y border-hairline py-8">
          <Spinner label="正在查询数据源…" />
        </div>
      ) : null}

      {!running && groups !== null ? (
        entries.length === 0 && groupsWithOutput.length === 0 ? (
          <EmptyState
            tone="block"
            title="没有查询到结果"
            description="确认词典已加载完成，或换一个词再试。也可以在「词典源」页检查该源的状态。"
          />
        ) : (
          <div className="flex flex-col gap-4">
            <p className="text-sm text-ink-muted">
              “{query.trim()}” · {entries.length} 条结果 · 最长耗时 {Math.round(maxElapsed)} ms
            </p>
            {failedGroups.length > 0 ? (
              <p className="text-xs text-ink-muted">
                另有 {failedGroups.length} 个源失败：
                {failedGroups
                  .map((group) => {
                    const failure = groupFailure(group);
                    return `${group.source.name}（${failure?.message ?? "未知错误"}）`;
                  })
                  .join("；")}
              </p>
            ) : null}
            {groupsWithOutput.map((group) => (
              <TestResultView
                key={group.source.id}
                result={group.result}
                error={group.error}
                name={group.source.name}
              />
            ))}
            {entries.length === 0 ? (
              <Button variant="secondary" size="sm" className="self-start" onClick={() => void run()}>
                重试
              </Button>
            ) : null}
          </div>
        )
      ) : null}

      {groups === null && !running && runError === null ? (
        <EmptyState
          title="输入词开始测试"
          description="查询测试会按当前配置直接读取源，用来验证本地词典或远程 API 是否可用。"
        />
      ) : null}
    </section>
  );
}
