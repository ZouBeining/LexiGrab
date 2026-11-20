// SPDX-License-Identifier: AGPL-3.0-only
import type { RemoteLookupError } from "../../lib/events";
import type { CachedSource } from "./state";

export interface RemoteStatusProps {
  pendingRemote: number;
  cachedSources: CachedSource[];
  errors: RemoteLookupError[];
}

/**
 * Remote query progress, cache-hit badges and the collapsed diagnostic list
 * (docs/APP_FLOW.md §6.1/§9, FRONTEND_GUIDELINES §4.3). Remote failures never
 * become a blocking modal; they only add entries to the 诊断 details.
 */
export function RemoteStatus({ pendingRemote, cachedSources, errors }: RemoteStatusProps) {
  if (pendingRemote <= 0 && cachedSources.length === 0 && errors.length === 0) {
    return null;
  }

  return (
    <section className="flex flex-col gap-2 border-t border-hairline pt-3" aria-label="远程查询状态">
      {pendingRemote > 0 ? (
        <div className="flex flex-col gap-1" role="status" aria-live="polite">
          <span className="text-[11px] leading-4 text-ink-muted">
            正在查询 {pendingRemote} 个远程源
          </span>
          <progress className="h-0.5 w-full accent-accent" aria-label="远程查询进度" />
        </div>
      ) : null}

      {cachedSources.length > 0 ? (
        <div className="flex flex-wrap items-center gap-2" aria-label="缓存命中来源">
          <span className="text-[11px] leading-4 text-ink-muted">缓存命中</span>
          {cachedSources.map((source) => (
            <span
              key={source.source_id}
              className="inline-flex items-center gap-1 rounded-full border border-hairline px-2 py-0.5 text-[10px] leading-4 text-ink"
            >
              <span className="truncate">{source.source_name}</span>
              <span className="text-ink-muted">缓存</span>
            </span>
          ))}
        </div>
      ) : null}

      {errors.length > 0 ? (
        <details className="text-[11px] leading-4 text-ink-muted">
          <summary className="cursor-pointer select-none rounded px-1 py-0.5 hover:text-ink">
            诊断（{errors.length}）
          </summary>
          <ul className="mt-1 flex flex-col gap-1 pl-3">
            {errors.map((error, index) => (
              <li key={`${error.source_id}:${error.code}:${index}`} className="break-words">
                <span className="font-mono tracking-[0.6px]">{error.code}</span>
                <span className="ml-1">{error.message}</span>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </section>
  );
}
