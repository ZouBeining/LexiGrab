import type { ReactNode } from "react";

export type EmptyStateTone = "plain" | "block";

const TONES: Record<EmptyStateTone, string> = {
  plain: "rounded-3xl border border-dashed border-hairline",
  block: "rounded-3xl bg-block-1",
};

export interface EmptyStateProps {
  title: string;
  description?: string;
  /** 空态引导动作（按钮/链接）；通常传一个 Button。 */
  action?: ReactNode;
  /** `block` 用 pastel 色块作为首次启动/空态引导背景，同一视图最多一个。 */
  tone?: EmptyStateTone;
  className?: string;
}

/** 空态：标题 + 说明 + 可选动作。 */
export function EmptyState({
  title,
  description,
  action,
  tone = "plain",
  className,
}: EmptyStateProps) {
  return (
    <div
      className={[
        "flex flex-col items-center justify-center gap-2 px-6 py-10 text-center",
        TONES[tone],
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      <p className="text-[16px] font-medium text-ink">{title}</p>
      {description ? <p className="max-w-[420px] text-sm text-ink-muted">{description}</p> : null}
      {action ? <div className="mt-2 flex flex-wrap items-center justify-center gap-2">{action}</div> : null}
    </div>
  );
}
