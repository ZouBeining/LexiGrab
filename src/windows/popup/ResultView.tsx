// SPDX-License-Identifier: AGPL-3.0-only
import type { KeyboardEvent } from "react";
import { SourceBadge } from "../../components/SourceBadge";
import type { Entry } from "../../lib/types";
import { DefinitionView } from "./DefinitionView";

export interface ResultViewProps {
  entries: Entry[];
  selectedEntryIndex: number;
  /** Entry that contains the recommended sense; marked with a text chip. */
  recommendedEntryIndex?: number | null;
  onSelectEntry?: (index: number) => void;
  className?: string;
}

/**
 * Dictionary entry list for the quick popup mode (docs/APP_FLOW.md §6.3).
 * All senses stay visible; the selected entry drives the save payload.
 */
export function ResultView({
  entries,
  selectedEntryIndex,
  recommendedEntryIndex = null,
  onSelectEntry,
  className,
}: ResultViewProps) {
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>, index: number) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelectEntry?.(index);
    }
  };
  return (
    <div className={["flex flex-col gap-2", className].filter(Boolean).join(" ")}>
      <ul role="radiogroup" aria-label="词典义项" className="divide-y divide-hairline">
        {entries.map((entry, index) => {
          const selected = index === selectedEntryIndex;
          const recommended = index === recommendedEntryIndex;
          return (
            <li key={`${entry.source_name}:${entry.headword}:${index}`}>
              <div
                role="radio"
                aria-checked={selected}
                aria-label={`${entry.headword} · ${entry.source_name}${
                  recommended ? "，语境推荐" : ""
                }`}
                tabIndex={0}
                onClick={() => onSelectEntry?.(index)}
                onKeyDown={(event) => handleKeyDown(event, index)}
                className={[
                  "cursor-pointer border-l-2 px-3 py-3 transition-colors duration-150",
                  selected
                    ? "border-l-accent bg-canvas-soft"
                    : "border-l-transparent hover:bg-canvas-soft/60",
                ].join(" ")}
              >
                <div className="mb-1 flex items-baseline justify-between gap-3">
                  <div className="flex min-w-0 items-baseline gap-2">
                    <span className="truncate text-[15px] font-semibold leading-6 text-ink">
                      {entry.headword}
                    </span>
                    {entry.phonetic ? (
                      <span className="truncate font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                        {entry.phonetic}
                      </span>
                    ) : null}
                    {recommended ? (
                      <span className="shrink-0 rounded-full border border-hairline px-1.5 text-[10px] leading-4 text-ink">
                        推荐
                      </span>
                    ) : null}
                  </div>
                  <SourceBadge name={entry.source_name} className="shrink-0" />
                </div>
                <DefinitionView html={entry.definition_html} />
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
