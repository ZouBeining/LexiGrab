export type SpinnerSize = "sm" | "md";

const SIZES: Record<SpinnerSize, string> = {
  sm: "size-4",
  md: "size-6",
};

export interface SpinnerProps {
  /** 可见的加载文案；省略时仅对读屏器播报“加载中”。 */
  label?: string;
  size?: SpinnerSize;
  className?: string;
}

/**
 * 加载指示。`prefers-reduced-motion` 下停止旋转但仍保留可见的环形指示，
 * 保证加载状态不依赖动画。
 */
export function Spinner({ label, size = "md", className }: SpinnerProps) {
  return (
    <span
      role="status"
      aria-live="polite"
      className={["inline-flex items-center gap-2", className].filter(Boolean).join(" ")}
    >
      <span
        aria-hidden="true"
        className={[
          SIZES[size],
          "rounded-full border-2 border-hairline border-t-accent",
          "animate-spin motion-reduce:animate-none",
        ].join(" ")}
      />
      {label ? (
        <span className="text-xs text-ink-muted">{label}</span>
      ) : (
        <span className="sr-only">加载中…</span>
      )}
    </span>
  );
}
