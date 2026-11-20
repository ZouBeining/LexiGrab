import { SourceBadge } from "../../components/SourceBadge";
import type { Entry, IpcError, TestResult } from "../../lib/types";

/** Backend already truncates to 4KB; slice again so the UI contract is explicit. */
const RAW_PREVIEW_LIMIT = 4096;

/** Converts sanitized definition HTML to short plain text for the test view. */
function plainText(html: string): string {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const text = (parsed.body.textContent ?? "").replace(/\s+/g, " ").trim();
  return text.length > 320 ? `${text.slice(0, 320)}…` : text;
}

interface EntryRowProps {
  entry: Entry;
  elapsedMs: number;
}

function EntryRow({ entry, elapsedMs }: EntryRowProps) {
  return (
    <li className="flex flex-col gap-1 border-b border-hairline py-3 last:border-b-0">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="text-sm font-medium">{entry.headword}</span>
        {entry.phonetic ? (
          <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">
            {entry.phonetic}
          </span>
        ) : null}
        <SourceBadge name={entry.source_name} />
        <span className="ml-auto font-mono text-[11px] tracking-[0.6px] text-ink-muted">
          {Math.round(elapsedMs)} ms
        </span>
      </div>
      <p className="text-sm leading-6 text-ink-muted">{plainText(entry.definition_html)}</p>
    </li>
  );
}

interface TestResultViewProps {
  /** Test payload for a saved source or a remote draft config. */
  result: TestResult | null;
  /** IPC/本地源错误；远程失败在 `result.error` 中。 */
  error: IpcError | null;
  /** Optional label when `source_name` is empty. */
  name?: string;
}

/**
 * Shared result panel for `test_source_query` (APP_FLOW §5.4 / PLAN_M4 T4.6):
 * entries, elapsed time, a readable error, and a collapsible raw response preview.
 */
export function TestResultView({ result, error, name }: TestResultViewProps) {
  if (error) {
    return (
      <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
        <p className="font-medium text-danger">测试失败</p>
        <p className="mt-1 text-ink-muted">{error.message}</p>
        <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{error.code}</p>
      </div>
    );
  }

  if (!result) return null;

  const preview =
    result.raw_preview !== null ? result.raw_preview.slice(0, RAW_PREVIEW_LIMIT) : null;
  const sourceLabel = result.source_name || name || "数据源";

  return (
    <section
      aria-label={`${sourceLabel} 的测试结果`}
      className="rounded-3xl border border-hairline px-5 py-4"
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-hairline pb-2">
        <div className="flex min-w-0 items-center gap-2">
          <SourceBadge name={sourceLabel} />
          <span className="text-xs text-ink-muted">{result.entries.length} 条结果</span>
        </div>
        <span className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">
          {Math.round(result.elapsed_ms)} ms
        </span>
      </div>

      {result.error ? (
        <div role="alert" className="mt-3 rounded-xl border border-danger/40 px-3 py-2 text-xs">
          <p className="font-medium text-danger">远程查询失败</p>
          <p className="mt-1 text-ink-muted">{result.error.message}</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
            {result.error.code}
          </p>
        </div>
      ) : null}

      {!result.error && result.entries.length === 0 ? (
        <p className="mt-3 text-sm text-ink-muted">没有查询到结果。</p>
      ) : null}

      {result.entries.length > 0 ? (
        <ul className="mt-1">
          {result.entries.map((entry, index) => (
            <EntryRow
              key={`${entry.headword}-${index}`}
              entry={entry}
              elapsedMs={result.elapsed_ms}
            />
          ))}
        </ul>
      ) : null}

      {preview !== null ? (
        <details className="mt-3 border-t border-hairline pt-3">
          <summary className="cursor-pointer text-xs text-accent">
            查看原始响应（前 4KB）
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto rounded-xl bg-canvas-soft p-3 font-mono text-[11px] leading-5 break-all whitespace-pre-wrap">
            {preview}
          </pre>
        </details>
      ) : null}
    </section>
  );
}
