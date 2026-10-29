import type { ButtonHTMLAttributes } from "react";

export type ButtonVariant = "primary" | "secondary" | "ghost";
export type ButtonSize = "md" | "sm";

const BASE =
  "inline-flex items-center justify-center gap-2 rounded-full font-medium transition-colors duration-150 " +
  "active:scale-[0.98] disabled:pointer-events-none disabled:opacity-48";

const VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-accent text-on-accent hover:bg-accent/90",
  secondary: "border border-hairline bg-canvas text-ink hover:bg-canvas-soft",
  ghost: "text-accent hover:bg-canvas-soft",
};

const SIZES: Record<ButtonSize, string> = {
  md: "h-10 px-5 text-sm",
  sm: "h-8 px-3 text-xs",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

/** 共享按钮。默认 primary pill；disabled 用 48% 透明度并禁止点击。 */
export function Button({
  variant = "primary",
  size = "md",
  type = "button",
  className,
  children,
  ...rest
}: ButtonProps) {
  const classes = [BASE, VARIANTS[variant], SIZES[size], className].filter(Boolean).join(" ");
  return (
    <button {...rest} type={type} className={classes}>
      {children}
    </button>
  );
}
