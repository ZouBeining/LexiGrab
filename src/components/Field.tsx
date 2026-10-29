import { cloneElement, isValidElement, useId, type ReactElement, type ReactNode } from "react";

/** 会被 Field 注入的控制属性。适用于 input / select / textarea。 */
interface DescribedControlProps {
  id?: string;
  "aria-describedby"?: string;
  "aria-invalid"?: boolean;
  "aria-required"?: boolean;
}

export interface FieldProps {
  /** 可见标签。 */
  label: string;
  /** 控件 id；省略时用 React useId 生成。 */
  id?: string;
  /** `id` 的别名，便于按 label 语义传参。 */
  htmlFor?: string;
  /** 辅助说明，错误时隐藏。 */
  hint?: string | null;
  /** 错误文案，显示在控件下方并与控件 aria-describedby 关联。 */
  error?: string | null;
  required?: boolean;
  className?: string;
  children: ReactNode;
}

/**
 * 表单字段外壳：label 在上、控件在中、提示/错误紧贴控件。
 * 若 children 是单个 React 元素，会自动注入 id 与 aria-describedby。
 */
export function Field({
  label,
  id,
  htmlFor,
  hint,
  error,
  required = false,
  className,
  children,
}: FieldProps) {
  const generatedId = useId();
  const childProps = isValidElement(children)
    ? (children.props as DescribedControlProps)
    : undefined;
  // 优先使用显式 id，其次沿用子控件已有 id，最后才生成。
  const controlId = id ?? htmlFor ?? childProps?.id ?? generatedId;
  const hintId = `${controlId}-hint`;
  const errorId = `${controlId}-error`;
  const describedBy = [
    childProps?.["aria-describedby"],
    hint && !error ? hintId : null,
    error ? errorId : null,
  ]
    .filter(Boolean)
    .join(" ");

  const control = isValidElement(children)
    ? cloneElement(children as ReactElement<DescribedControlProps>, {
        id: controlId,
        "aria-describedby": describedBy.length > 0 ? describedBy : undefined,
        "aria-invalid": error ? true : undefined,
        "aria-required": required ? true : undefined,
      })
    : children;

  return (
    <div className={["flex flex-col gap-2", className].filter(Boolean).join(" ")}>
      <label htmlFor={controlId} className="text-sm font-medium text-ink">
        {label}
        {required ? (
          <span aria-hidden="true" className="ml-1 text-danger">
            *
          </span>
        ) : null}
      </label>
      {control}
      {hint && !error ? (
        <p id={hintId} className="text-xs leading-4 text-ink-muted">
          {hint}
        </p>
      ) : null}
      {error ? (
        <p id={errorId} role="alert" className="flex items-start gap-1 text-xs leading-4 text-danger">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </p>
      ) : null}
    </div>
  );
}
