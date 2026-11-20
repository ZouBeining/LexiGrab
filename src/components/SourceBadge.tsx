// SPDX-License-Identifier: AGPL-3.0-only
export interface SourceBadgeProps {
  /** 来源名；与 `source` 二选一，优先使用 `name`。 */
  name?: string;
  /** `name` 的别名。 */
  source?: string;
  className?: string;
}

/** 来源徽标：等宽小字，不加彩色背景。 */
export function SourceBadge({ name, source, className }: SourceBadgeProps) {
  const text = name ?? source;
  if (!text) return null;
  return (
    <span
      className={[
        "inline-flex items-center font-mono text-[11px] leading-4 tracking-[0.6px] text-ink-muted",
        className,
      ]
        .filter(Boolean)
        .join(" ")}
    >
      {text}
    </span>
  );
}
