// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { updateSettings, toIpcError } from "../../lib/ipc";
import type {
  GeneralSettings,
  IpcError,
  QueryStrategy,
  Settings,
  ThemePreference,
} from "../../lib/types";
import { applyTheme, DEFAULT_THEME, isTheme, storeTheme } from "../../lib/theme";
import { Button } from "../../components/Button";
import { Field } from "../../components/Field";
import { HotkeyInput } from "./HotkeyInput";

const THEME_OPTIONS: { value: ThemePreference; label: string; description: string }[] = [
  { value: "figma", label: "Figma（默认）", description: "黑白核心与 pastel 色块。" },
  { value: "apple", label: "Apple", description: "Action Blue 与 parchment 冷色。" },
  { value: "cursor", label: "Cursor", description: "橙色强调与暖白画布。" },
];

const STRATEGY_OPTIONS: {
  value: QueryStrategy;
  label: string;
  description: string;
}[] = [
  {
    value: "local_first",
    label: "本地优先",
    description: "先返回本地结果，远程结果到达后追加。",
  },
  {
    value: "first_hit",
    label: "首个命中",
    description: "按优先级使用第一个命中的源，缩短等待。",
  },
  {
    value: "aggregate",
    label: "聚合全部",
    description: "合并所有启用源的结果，再按条数上限截断。",
  },
];

/** Normalizes historical `system` to the default theme for radio selection. */
function effectiveTheme(theme: ThemePreference): ThemePreference {
  return isTheme(theme) ? theme : DEFAULT_THEME;
}

interface FieldErrors {
  hotkey?: string;
  perSource?: string;
  total?: string;
}

interface GeneralSectionProps {
  settings: Settings;
  onSaved: (next: Settings) => void;
}

/**
 * General settings (docs/APP_FLOW.md §7.1): hotkey recorder, theme preview,
 * query strategy, entry limits and startup behavior. Save applies and persists.
 */
export function GeneralSection({ settings, onSaved }: GeneralSectionProps) {
  const [draft, setDraft] = useState<GeneralSettings>({
    ...settings.general,
    theme: effectiveTheme(settings.general.theme),
  });
  const [errors, setErrors] = useState<FieldErrors>({});
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  // Theme previews immediately by swapping the CSS variable set (no layout change).
  useEffect(() => {
    if (isTheme(draft.theme)) applyTheme(draft.theme);
  }, [draft.theme]);

  const update = <K extends keyof GeneralSettings>(key: K, value: GeneralSettings[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setSaved(false);
    setSaveError(null);
  };

  const validate = (): FieldErrors => {
    const next: FieldErrors = {};
    if (draft.hotkey.trim().length === 0) {
      next.hotkey = "请录制一个全局热键。";
    } else if (!/(CmdOrCtrl|Alt|Shift|Super)/.test(draft.hotkey)) {
      next.hotkey = "热键必须包含 Ctrl、Alt 或 Shift 修饰键。";
    }
    const perSource = draft.max_entries_per_source;
    if (!Number.isInteger(perSource) || perSource < 1 || perSource > 10) {
      next.perSource = "每个源的条数需为 1–10 的整数。";
    }
    const total = draft.max_entries_total;
    if (!Number.isInteger(total) || total < 5 || total > 50) {
      next.total = "结果总条数需为 5–50 的整数。";
    }
    return next;
  };

  const save = async () => {
    if (saving) return;
    const nextErrors = validate();
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0) return;

    setSaving(true);
    setSaveError(null);
    try {
      const updated = await updateSettings({ ...settings, general: draft });
      if (isTheme(draft.theme)) storeTheme(draft.theme);
      onSaved(updated);
      // Mirror the persisted values so the draft stays in sync without an effect.
      setDraft({ ...updated.general, theme: effectiveTheme(updated.general.theme) });
      setSaved(true);
    } catch (cause: unknown) {
      const error = toIpcError(cause);
      if (error.code === "HOTKEY_CONFLICT") {
        // Rollback to the persisted hotkey; the old binding stays active.
        setDraft((current) => ({ ...current, hotkey: settings.general.hotkey }));
        setErrors((current) => ({ ...current, hotkey: error.message }));
      } else {
        setSaveError(error);
      }
      const persistedTheme = effectiveTheme(settings.general.theme);
      if (isTheme(persistedTheme)) applyTheme(persistedTheme);
    } finally {
      setSaving(false);
    }
  };

  const dirty = JSON.stringify(draft) !== JSON.stringify(settings.general);

  return (
    <section aria-labelledby="general-title" className="flex min-h-full max-w-[560px] flex-col">
      <header>
        <h1 id="general-title" className="text-[26px] font-medium tracking-tight">
          通用
        </h1>
        <p className="mt-1 text-sm text-ink-muted">热键、主题、查询策略与启动行为。</p>
      </header>

      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="mt-8 flex flex-1 flex-col gap-8"
      >
        <Field
          label="全局热键"
          id="settings-hotkey"
          hint="保存时后端尝试注册；冲突会保留旧热键并提示。"
          error={errors.hotkey ?? null}
        >
          <HotkeyInput
            value={draft.hotkey}
            onChange={(value) => {
              update("hotkey", value);
              setErrors((current) => ({ ...current, hotkey: undefined }));
            }}
          />
        </Field>

        <fieldset className="flex flex-col gap-3">
          <legend className="text-sm font-medium">主题</legend>
          <div role="radiogroup" aria-label="主题" className="flex flex-col gap-2">
            {THEME_OPTIONS.map((option) => {
              const checked = draft.theme === option.value;
              return (
                <label
                  key={option.value}
                  className={`flex cursor-pointer items-center justify-between gap-4 rounded-xl border px-4 py-3 text-sm transition-colors duration-150 ${
                    checked ? "border-accent" : "border-hairline hover:bg-canvas-soft"
                  }`}
                >
                  <span>
                    <span className="block font-medium">{option.label}</span>
                    <span className="mt-0.5 block text-xs text-ink-muted">{option.description}</span>
                  </span>
                  <input
                    type="radio"
                    name="theme"
                    value={option.value}
                    checked={checked}
                    onChange={() => update("theme", option.value)}
                    className="size-4 accent-accent"
                  />
                </label>
              );
            })}
          </div>
          <p className="text-xs text-ink-muted">
            主题立即预览，点击保存后持久化；0.1.0 只提供浅色主题。
          </p>
        </fieldset>

        <fieldset className="flex flex-col gap-3">
          <legend className="text-sm font-medium">查询策略</legend>
          <div role="radiogroup" aria-label="查询策略" className="flex flex-col gap-2">
            {STRATEGY_OPTIONS.map((option) => {
              const checked = draft.query_strategy === option.value;
              return (
                <label
                  key={option.value}
                  className={`flex cursor-pointer items-center justify-between gap-4 rounded-xl border px-4 py-3 text-sm transition-colors duration-150 ${
                    checked ? "border-accent" : "border-hairline hover:bg-canvas-soft"
                  }`}
                >
                  <span>
                    <span className="block font-medium">{option.label}</span>
                    <span className="mt-0.5 block text-xs text-ink-muted">{option.description}</span>
                  </span>
                  <input
                    type="radio"
                    name="query-strategy"
                    value={option.value}
                    checked={checked}
                    onChange={() => update("query_strategy", option.value)}
                    className="size-4 accent-accent"
                  />
                </label>
              );
            })}
          </div>
        </fieldset>

        <div className="grid grid-cols-2 gap-4">
          <Field
            label="每个源最多条数"
            id="settings-per-source"
            hint="范围 1–10。"
            error={errors.perSource ?? null}
          >
            <input
              inputMode="numeric"
              value={draft.max_entries_per_source}
              onChange={(event) => {
                update("max_entries_per_source", Number.parseInt(event.target.value, 10) || 0);
                setErrors((current) => ({ ...current, perSource: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            />
          </Field>
          <Field
            label="结果总条数"
            id="settings-total"
            hint="范围 5–50。"
            error={errors.total ?? null}
          >
            <input
              inputMode="numeric"
              value={draft.max_entries_total}
              onChange={(event) => {
                update("max_entries_total", Number.parseInt(event.target.value, 10) || 0);
                setErrors((current) => ({ ...current, total: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            />
          </Field>
        </div>

        <label className="flex items-start gap-3 text-sm">
          <input
            type="checkbox"
            checked={draft.start_minimized}
            onChange={(event) => update("start_minimized", event.target.checked)}
            className="mt-0.5 size-4 accent-accent"
          />
          <span>
            <span className="block font-medium">启动时最小化</span>
            <span className="mt-0.5 block text-xs text-ink-muted">
              下次启动生效；应用仍在托盘中运行。
            </span>
          </span>
        </label>

        {saveError ? (
          <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {saveError.code}
            </p>
          </div>
        ) : null}

        <footer className="sticky bottom-0 mt-auto flex items-center justify-between gap-4 border-t border-hairline bg-canvas py-4">
          <p role="status" aria-live="polite" className="text-xs text-ink-muted">
            {saved ? "已保存" : dirty ? "有未保存的更改" : ""}
          </p>
          <Button type="submit" disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </Button>
        </footer>
      </form>
    </section>
  );
}
