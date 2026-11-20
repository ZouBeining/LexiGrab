// SPDX-License-Identifier: AGPL-3.0-only
import { Button } from "../../components/Button";
import type { AiDefinition, SenseCandidate, SenseResolution, SenseStatus } from "../../lib/types";

/** Human-readable status copy for the recommendation bar (docs/APP_FLOW.md §6.6). */
export function senseStatusText(status: SenseStatus): string | null {
  switch (status) {
    case "selected":
      return "推荐义项";
    case "ambiguous":
      return "建议确认";
    case "low_confidence":
      return "可能不在词典义项中";
    case "unavailable":
      return "语境模型不可用";
    case "failed":
      return "语境辅助失败";
    case "no_context":
      return "缺少原句";
    case "disabled":
      return null;
    default:
      return null;
  }
}

function ConfidenceMeter({ confidence }: { confidence: number }) {
  const clamped = Math.min(1, Math.max(0, confidence));
  return (
    <div className="flex min-w-[140px] flex-1 flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2">
        <span className="font-mono text-[11px] tracking-[0.6px] text-ink">{clamped.toFixed(2)}</span>
        <span className="text-[11px] leading-4 text-ink-muted">模型自评</span>
      </div>
      <progress
        className="h-1 w-full accent-accent"
        aria-label="置信度进度"
        aria-valuetext={`${clamped.toFixed(2)}，模型自评`}
        value={clamped}
        max={1}
      />
    </div>
  );
}

export interface SenseCandidateListProps {
  candidates: SenseCandidate[];
  recommendedSenseId: string | null;
  selectedSenseId: string | null;
  onSelectSense: (senseId: string) => void;
}

/**
 * Switch list for sense candidates. Every candidate stays visible; the
 * recommendation is marked in text (not only by color) and the user's active
 * choice is announced through aria-checked.
 */
export function SenseCandidateList({
  candidates,
  recommendedSenseId,
  selectedSenseId,
  onSelectSense,
}: SenseCandidateListProps) {
  if (candidates.length === 0) {
    return null;
  }
  return (
    <div className="mt-2 flex flex-col gap-1" role="radiogroup" aria-label="义项列表">
      {candidates.map((candidate) => {
        const selected = candidate.id === selectedSenseId;
        const recommended = candidate.id === recommendedSenseId;
        return (
          <button
            key={candidate.id}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={`${candidate.part_of_speech ?? "未标注词性"}：${candidate.definition_text}${
              recommended ? "，语境推荐" : ""
            }`}
            onClick={() => onSelectSense(candidate.id)}
            className={[
              "flex w-full items-start gap-2 rounded-lg border-l-2 px-2 py-1.5 text-left text-xs leading-5 transition-colors duration-150",
              selected
                ? "border-l-accent bg-canvas-soft text-ink"
                : "border-l-transparent text-ink-muted hover:bg-canvas-soft/60 hover:text-ink",
            ].join(" ")}
          >
            {recommended ? (
              <span className="shrink-0 rounded-full border border-hairline px-1.5 text-[10px] leading-4 text-ink">
                推荐
              </span>
            ) : null}
            {candidate.part_of_speech ? (
              <span className="shrink-0 font-mono text-[10px] leading-5 tracking-[0.6px] text-ink-muted">
                {candidate.part_of_speech}
              </span>
            ) : null}
            <span className="min-w-0 flex-1">{candidate.definition_text}</span>
            {selected ? (
              <span className="shrink-0 text-[10px] leading-5 text-accent" aria-hidden="true">
                已选
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}

export interface AiCardProps {
  ai: AiDefinition;
  /** True once the AI text has been adopted and the editor opened. */
  aiInUse: boolean;
  onUse: () => void;
}

/**
 * AI fallback card. It never becomes the active definition on its own: the user
 * must press "使用此释义" (docs/APP_FLOW.md §6.6, FRONTEND_GUIDELINES §5.6).
 */
export function AiCard({ ai, aiInUse, onUse }: AiCardProps) {
  return (
    <section
      aria-label="AI 释义"
      className="rounded-xl border border-dashed border-hairline bg-canvas-soft p-3"
    >
      <p className="font-mono text-[11px] leading-4 tracking-[0.6px] text-ink-muted">
        AI 生成 · {ai.model} · 可能不准确
      </p>
      {ai.part_of_speech ? (
        <p className="mt-1 font-mono text-[10px] leading-4 tracking-[0.6px] text-ink-muted">
          {ai.part_of_speech}
        </p>
      ) : null}
      <p className="mt-1 text-sm leading-5 text-ink">{ai.definition_text}</p>
      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="text-[11px] leading-4 text-ink-muted">
          {aiInUse ? "已填入编辑区，可继续修改。" : "默认未采用，确认后才会用于制卡。"}
        </span>
        <Button size="sm" variant="secondary" onClick={onUse} disabled={aiInUse}>
          {aiInUse ? "已使用此释义" : "使用此释义"}
        </Button>
      </div>
    </section>
  );
}

export interface SenseAssistantProps {
  resolution: SenseResolution | null;
  /** No original sentence was captured; the bar explains why no request was sent. */
  sentenceMissing: boolean;
  selectedSenseId: string | null;
  aiInUse: boolean;
  onSelectSense: (senseId: string) => void;
  onUseAi: () => void;
}

/**
 * Recommendation strip + candidate switcher + optional AI card. Failure states
 * live only here; the dictionary entries and the save button are untouched.
 */
export function SenseAssistant({
  resolution,
  sentenceMissing,
  selectedSenseId,
  aiInUse,
  onSelectSense,
  onUseAi,
}: SenseAssistantProps) {
  const statusText = sentenceMissing
    ? "缺少原句"
    : resolution
      ? senseStatusText(resolution.status)
      : "正在判断语境…";

  if (!statusText) {
    return null;
  }

  const confidence = resolution?.confidence ?? null;
  const recommendedId = resolution?.selected_sense_id ?? null;
  const ai = resolution?.ai ?? null;
  const showAi =
    ai !== null &&
    (resolution?.status === "low_confidence" ||
      resolution?.status === "failed" ||
      (resolution?.status === "selected" &&
        !resolution.candidates.some((candidate) => candidate.id === recommendedId)));

  return (
    <section className="flex flex-col gap-2" aria-label="语境推荐">
      <div className="rounded-lg bg-canvas-soft px-3 py-2" role="note">
        <div className="flex items-center justify-between gap-2">
          <p className="text-xs font-medium leading-5 text-ink">{statusText}</p>
          {resolution?.selector_model ? (
            <span className="truncate font-mono text-[10px] tracking-[0.6px] text-ink-muted">
              {resolution.selector_model}
            </span>
          ) : null}
        </div>
        {confidence !== null ? <ConfidenceMeter confidence={confidence} /> : null}
        {!confidence && !sentenceMissing && resolution && resolution.status !== "disabled" ? (
          <p className="mt-1 text-[11px] leading-4 text-ink-muted">未返回置信度，可继续手动选择义项。</p>
        ) : null}
      </div>

      <SenseCandidateList
        candidates={resolution?.candidates ?? []}
        recommendedSenseId={recommendedId}
        selectedSenseId={selectedSenseId}
        onSelectSense={onSelectSense}
      />

      {showAi && ai ? <AiCard ai={ai} aiInUse={aiInUse} onUse={onUseAi} /> : null}
    </section>
  );
}
