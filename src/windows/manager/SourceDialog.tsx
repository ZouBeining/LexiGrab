import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { addSource, toIpcError, updateSource } from "../../lib/ipc";
import type { IpcError, SourceConfig } from "../../lib/types";
import { Modal } from "./Modal";
import { basename, normalisePath, stripMdxExtension } from "./paths";

interface SourceDialogProps {
  mode: "add" | "edit";
  /** Existing source when editing; null when adding. */
  source: SourceConfig | null;
  /** Used for duplicate-path validation and priority defaults. */
  existingSources: SourceConfig[];
  onClose: () => void;
  /** Called after a successful add/update; the caller reloads the list. */
  onSaved: () => void;
}

interface FieldErrors {
  name?: string;
  paths?: string;
  priority?: string;
}

function defaultPriority(sources: SourceConfig[]): number {
  const max = sources.reduce((highest, item) => Math.max(highest, item.priority), 0);
  return max + 10;
}

/**
 * Add/edit dialog for local MDX sources (docs/APP_FLOW.md §5.2).
 * The backend generates ids; this form only submits name, paths, enabled and priority.
 */
export function SourceDialog({ mode, source, existingSources, onClose, onSaved }: SourceDialogProps) {
  const [name, setName] = useState(source?.name ?? "");
  const [paths, setPaths] = useState<string[]>(source?.paths ?? []);
  const [enabled, setEnabled] = useState(source?.enabled ?? true);
  const [priority, setPriority] = useState(
    String(source?.priority ?? defaultPriority(existingSources)),
  );
  const [nameAuto, setNameAuto] = useState(mode === "add");
  const [fieldErrors, setFieldErrors] = useState<FieldErrors>({});
  const [pickerError, setPickerError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);

  const nameRef = useRef<HTMLInputElement>(null);
  const fileButtonRef = useRef<HTMLButtonElement>(null);
  const priorityRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (mode === "add") fileButtonRef.current?.focus();
    else nameRef.current?.focus();
  }, [mode]);

  const pickFiles = async () => {
    setPickerError(null);
    try {
      const selection = await open({
        multiple: true,
        filters: [{ name: "MDX 词典", extensions: ["mdx"] }],
      });
      if (!selection) return;
      const picked = (Array.isArray(selection) ? selection : [selection]).filter(
        (item): item is string => typeof item === "string" && item.length > 0,
      );
      if (picked.length === 0) return;
      setPaths((current) => {
        const seen = new Set(current.map(normalisePath));
        const next = [...current];
        for (const item of picked) {
          if (!seen.has(normalisePath(item))) {
            seen.add(normalisePath(item));
            next.push(item);
          }
        }
        return next;
      });
      setFieldErrors((current) => ({ ...current, paths: undefined }));
      if (nameAuto) {
        const first = picked[0];
        if (first) setName(stripMdxExtension(basename(first)));
      }
    } catch (cause: unknown) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setPickerError(`无法打开文件选择器：${message}`);
    }
  };

  const removePath = (path: string) => {
    setPaths((current) => current.filter((item) => item !== path));
  };

  const validate = (): FieldErrors => {
    const errors: FieldErrors = {};
    const trimmedName = name.trim();
    if (trimmedName.length === 0) {
      errors.name = "请输入词典名称。";
    } else if (trimmedName.length > 80) {
      errors.name = "词典名称不能超过 80 个字符。";
    }

    if (paths.length === 0) {
      errors.paths = "请选择至少一个 .mdx 文件。";
    } else {
      const invalidExtension = paths.find((path) => !/\.mdx$/i.test(path));
      if (invalidExtension) {
        errors.paths = `只支持 .mdx 文件：${basename(invalidExtension)}。`;
      } else {
        const others = existingSources.filter((item) => item.id !== source?.id);
        for (const path of paths) {
          const conflict = others.find((item) =>
            item.paths.some((existing) => normalisePath(existing) === normalisePath(path)),
          );
          if (conflict) {
            errors.paths = `该文件已被「${conflict.name}」使用：${basename(path)}。`;
            break;
          }
        }
      }
    }

    const priorityValue = Number.parseInt(priority, 10);
    if (!Number.isInteger(priorityValue) || priorityValue < 1 || priorityValue > 9999) {
      errors.priority = "优先级需为 1–9999 的整数。";
    }

    return errors;
  };

  const focusFirstError = (errors: FieldErrors) => {
    if (errors.name) nameRef.current?.focus();
    else if (errors.paths) fileButtonRef.current?.focus();
    else if (errors.priority) priorityRef.current?.focus();
  };

  const submit = async () => {
    if (saving) return;
    const errors = validate();
    setFieldErrors(errors);
    if (Object.keys(errors).length > 0) {
      focusFirstError(errors);
      return;
    }

    const config: SourceConfig = {
      id: mode === "edit" && source ? source.id : "",
      name: name.trim(),
      type: "local_file",
      paths,
      enabled,
      priority: Number.parseInt(priority, 10),
    };

    setSaving(true);
    setSaveError(null);
    try {
      if (mode === "edit" && source) {
        await updateSource(source.id, config);
      } else {
        await addSource(config);
      }
      onSaved();
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      labelledBy="source-dialog-title"
      onClose={onClose}
      initialFocus={mode === "add" ? fileButtonRef : nameRef}
    >
      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <h2 id="source-dialog-title" className="text-[16px] font-medium">
              {mode === "edit" ? "编辑词典源" : "添加本地词典"}
            </h2>
            <p className="mt-1 text-sm text-ink-muted">
              {mode === "edit"
                ? "修改后保存会重新加载该词典。"
                : "支持多选 .mdx；同名 .mdd 会自动配对。"}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭对话框"
            className="flex size-8 shrink-0 items-center justify-center rounded-full text-ink-muted transition-colors duration-150 hover:bg-canvas-soft hover:text-ink"
          >
            <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4" fill="none">
              <path d="M4 4l8 8M12 4l-8 8" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
          </button>
        </div>

        {saveError ? (
          <div role="alert" className="mt-4 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{saveError.code}</p>
          </div>
        ) : null}

        <div className="mt-5 flex flex-col gap-5">
          <div className="flex flex-col gap-2">
            <span className="text-sm font-medium">
              词典文件<span className="ml-1 text-danger">*</span>
            </span>
            <button
              ref={fileButtonRef}
              type="button"
              onClick={() => void pickFiles()}
              aria-describedby="source-paths-hint source-paths-error"
              className="h-10 self-start rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft"
            >
              选择 .mdx 文件
            </button>
            <p id="source-paths-hint" className="text-xs text-ink-muted">
              选择多个文件时按顺序加载；同名 .mdd 图片与音频会自动生效。
            </p>
            {paths.length > 0 ? (
              <ul className="flex flex-col gap-1">
                {paths.map((path) => (
                  <li
                    key={path}
                    className="flex items-center justify-between gap-3 rounded-xl bg-canvas-soft px-3 py-2"
                  >
                    <span className="min-w-0 truncate font-mono text-xs" title={path}>
                      {basename(path)}
                    </span>
                    <button
                      type="button"
                      onClick={() => removePath(path)}
                      aria-label={`移除 ${basename(path)}`}
                      className="shrink-0 text-xs text-ink-muted transition-colors duration-150 hover:text-danger"
                    >
                      移除
                    </button>
                  </li>
                ))}
              </ul>
            ) : null}
            {fieldErrors.paths ? (
              <p id="source-paths-error" role="alert" className="text-xs text-danger">
                {fieldErrors.paths}
              </p>
            ) : null}
            {pickerError ? (
              <p role="alert" className="text-xs text-danger">
                {pickerError}
              </p>
            ) : null}
          </div>

          <div className="flex flex-col gap-2">
            <label htmlFor="source-name" className="text-sm font-medium">
              名称<span className="ml-1 text-danger">*</span>
            </label>
            <input
              ref={nameRef}
              id="source-name"
              value={name}
              onChange={(event) => {
                setName(event.target.value);
                setNameAuto(false);
                setFieldErrors((current) => ({ ...current, name: undefined }));
              }}
              aria-invalid={fieldErrors.name ? true : undefined}
              aria-describedby={fieldErrors.name ? "source-name-error" : undefined}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              placeholder="例如：牛津高阶"
            />
            {fieldErrors.name ? (
              <p id="source-name-error" role="alert" className="text-xs text-danger">
                {fieldErrors.name}
              </p>
            ) : null}
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div className="flex flex-col gap-2">
              <label htmlFor="source-priority" className="text-sm font-medium">
                优先级
              </label>
              <input
                ref={priorityRef}
                id="source-priority"
                inputMode="numeric"
                value={priority}
                onChange={(event) => {
                  setPriority(event.target.value);
                  setFieldErrors((current) => ({ ...current, priority: undefined }));
                }}
                aria-invalid={fieldErrors.priority ? true : undefined}
                aria-describedby={fieldErrors.priority ? "source-priority-error" : undefined}
                className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
              />
            </div>
            <label className="flex items-end gap-2 pb-2 text-sm">
              <input
                type="checkbox"
                checked={enabled}
                onChange={(event) => setEnabled(event.target.checked)}
                className="size-4 accent-accent"
              />
              启用此源
            </label>
          </div>
          <p className="text-xs text-ink-muted">数值越小越优先；列表排序会重写为 10、20、30…。</p>
          {fieldErrors.priority ? (
            <p id="source-priority-error" role="alert" className="text-xs text-danger">
              {fieldErrors.priority}
            </p>
          ) : null}
        </div>

        <div className="mt-6 flex items-center justify-end gap-3 border-t border-hairline pt-4">
          <button
            type="button"
            onClick={onClose}
            className="h-10 rounded-full border border-hairline px-5 text-sm transition-colors duration-150 hover:bg-canvas-soft"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={saving}
            className="h-10 rounded-full bg-accent px-5 text-sm text-on-accent transition-transform duration-150 active:scale-[0.98] disabled:opacity-50"
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
