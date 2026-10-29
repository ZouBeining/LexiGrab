import type { HTMLAttributes } from "react";

export type CardPadding = "none" | "sm" | "md";

const PADDING: Record<CardPadding, string> = {
  none: "",
  sm: "p-4",
  md: "px-6 py-5",
};

export interface CardProps extends HTMLAttributes<HTMLDivElement> {
  padding?: CardPadding;
}

/** 共享卡片：24px 圆角 + hairline，无阴影。 */
export function Card({ padding = "md", className, children, ...rest }: CardProps) {
  const classes = [
    "rounded-3xl border border-hairline bg-canvas",
    PADDING[padding],
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div {...rest} className={classes}>
      {children}
    </div>
  );
}
