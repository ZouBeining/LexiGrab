// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";
import {
  getAnkiDecks,
  getAnkiModels,
  probeAnki,
  toIpcError,
  updateSettings,
} from "../../lib/ipc";
import type {
  AnkiFieldMap,
  AnkiModel,
  AnkiSettings,
  AnkiStatus,
  IpcError,
  Settings,
} from "../../lib/types";
import { Button } from "../../components/Button";
import { Field } from "../../components/Field";
import { Spinner } from "../../components/Spinner";

type FieldKey = keyof AnkiFieldMap;

const FIELD_LABELS: { key: FieldKey; label: string }[] = [
  { key: "word", label: "单词" },
  { key: "definition", label: "释义" },
  { key: "sentence", label: "原句" },
  { key: "source", label: "来源" },
];

type MappingErrors = Partial<Record<FieldKey, string>>;

interface AnkiFieldErrors {
  endpoint?: string;
  deck?: string;
  model?: string;
  tags?: string;
  mapping: MappingErrors;
}

interface AnkiSectionProps {
  settings: Settings;
  onSaved: (next: Settings) => void;
}

function parseTags(text: string): string[] {
  return text
    .split(/[,，]/)
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

/**
 * Anki settings (docs/APP_FLOW.md §7.2): connection probe, decks/models,
 * four field mappings, tags and duplicate policy. The test action is read-only.
 */
export function AnkiSection({ settings, onSaved }: AnkiSectionProps) {
  const [draft, setDraft] = useState<AnkiSettings>({ ...settings.anki });
  const [tagsText, setTagsText] = useState(settings.anki.tags.join(", "));
  const [probe, setProbe] = useState<AnkiStatus | null>(null);
  const [probing, setProbing] = useState(true);
  const [probeError, setProbeError] = useState<IpcError | null>(null);
  const [decks, setDecks] = useState<string[] | null>(null);
  const [models, setModels] = useState<AnkiModel[] | null>(null);
  const [remoteError, setRemoteError] = useState<IpcError | null>(null);
  const [loadingRemote, setLoadingRemote] = useState(false);
  const [errors, setErrors] = useState<AnkiFieldErrors>({ mapping: {} });
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const loadRemoteData = useCallback(async () => {
    setLoadingRemote(true);
    setRemoteError(null);
    try {
      const [deckList, modelList] = await Promise.all([getAnkiDecks(), getAnkiModels()]);
      setDecks(deckList);
      setModels(modelList);
    } catch (cause: unknown) {
      setRemoteError(toIpcError(cause));
    } finally {
      setLoadingRemote(false);
    }
  }, []);

  /** Read-only connection probe; also refreshes decks and models when available. */
  const runProbe = useCallback(async () => {
    try {
      const status = await probeAnki();
      setProbe(status);
      if (status.available) await loadRemoteData();
    } catch (cause: unknown) {
      setProbeError(toIpcError(cause));
    } finally {
      setProbing(false);
    }
  }, [loadRemoteData]);

  useEffect(() => {
    let active = true;
    probeAnki()
      .then((status) => {
        if (!active) return;
        setProbe(status);
        if (status.available) return loadRemoteData();
        return undefined;
      })
      .catch((cause: unknown) => {
        if (active) setProbeError(toIpcError(cause));
      })
      .finally(() => {
        if (active) setProbing(false);
      });
    return () => {
      active = false;
    };
  }, [loadRemoteData]);

  const testConnection = () => {
    setProbing(true);
    setProbeError(null);
    void runProbe();
  };

  const selectedModel = models?.find((model) => model.name === draft.model) ?? null;
  const availableFields = selectedModel?.fields ?? [];
  const deckOptions =
    decks !== null && !decks.includes(draft.deck) && draft.deck.length > 0
      ? [draft.deck, ...decks]
      : (decks ?? []);
  const modelOptions =
    models !== null && !models.some((model) => model.name === draft.model) && draft.model.length > 0
      ? [draft.model, ...models.map((model) => model.name)]
      : (models?.map((model) => model.name) ?? []);

  const updateFieldMap = (key: FieldKey, value: string) => {
    setDraft((current) => ({
      ...current,
      field_map: { ...current.field_map, [key]: value },
    }));
    setErrors((current) => ({
      ...current,
      mapping: { ...current.mapping, [key]: undefined },
    }));
    setSaved(false);
  };

  const validate = (): AnkiFieldErrors => {
    const next: AnkiFieldErrors = { mapping: {} };
    if (!/^https?:\/\//i.test(draft.endpoint.trim())) {
      next.endpoint = "请输入以 http:// 或 https:// 开头的 AnkiConnect 地址。";
    }
    if (decks !== null && draft.deck.trim().length === 0) {
      next.deck = "请选择牌组。";
    }
    if (models !== null && draft.model.trim().length === 0) {
      next.model = "请选择笔记类型。";
    }
    for (const { key } of FIELD_LABELS) {
      const value = draft.field_map[key].trim();
      if (value.length === 0) {
        next.mapping[key] = "请选择字段。";
      } else if (availableFields.length > 0 && !availableFields.includes(value)) {
        next.mapping[key] = "该字段不在所选笔记类型中。";
      }
    }
    return next;
  };

  const save = async () => {
    if (saving) return;
    const nextErrors = validate();
    setErrors(nextErrors);
    const hasMappingError = Object.keys(nextErrors.mapping).length > 0;
    if (nextErrors.endpoint || nextErrors.deck || nextErrors.model || hasMappingError) return;

    setSaving(true);
    setSaveError(null);
    try {
      const updated = await updateSettings({
        ...settings,
        anki: { ...draft, tags: parseTags(tagsText) },
      });
      onSaved(updated);
      // Mirror the persisted values so the draft stays in sync without an effect.
      setDraft({ ...updated.anki });
      setTagsText(updated.anki.tags.join(", "));
      setSaved(true);
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  const dirty =
    JSON.stringify({ ...draft, tags: parseTags(tagsText) }) !== JSON.stringify(settings.anki);

  return (
    <section aria-labelledby="anki-title" className="flex min-h-full max-w-[560px] flex-col">
      <header>
        <h1 id="anki-title" className="text-[26px] font-medium tracking-tight">
          Anki
        </h1>
        <p className="mt-1 text-sm text-ink-muted">
          通过 AnkiConnect 写入卡片；牌组、笔记类型与字段映射在导出前校验。
        </p>
      </header>

      <div className="mt-6 flex items-center justify-between gap-4 rounded-2xl bg-canvas-soft px-5 py-4">
        <div className="min-w-0">
          {probing ? (
            <Spinner label="正在探测 AnkiConnect…" />
          ) : probeError ? (
            <p role="alert" className="text-sm text-danger">
              探测失败：{probeError.message}
            </p>
          ) : probe?.available ? (
            <p className="text-sm">
              <span className="font-medium">AnkiConnect 可用</span>
              {probe.version !== null ? (
                <span className="ml-2 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                  v{probe.version}
                </span>
              ) : null}
            </p>
          ) : (
            <p className="text-sm text-ink-muted">
              Anki 未连接：{probe?.message ?? "请确认 Anki 已启动。"}
            </p>
          )}
        </div>
        <Button variant="secondary" size="sm" onClick={() => void testConnection()} disabled={probing}>
          测试连接
        </Button>
      </div>

      {!probing && (probe === null || !probe.available) ? (
        <div className="mt-4 rounded-2xl border border-hairline px-5 py-4">
          <p className="text-sm font-medium">连接 Anki 的三步</p>
          <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5 text-sm text-ink-muted">
            <li>安装并启动 Anki 桌面版。</li>
            <li>在 Anki 中安装 AnkiConnect 插件，并保持 Anki 运行。</li>
            <li>回到这里点击“测试连接”；成功后再选择牌组与字段映射。</li>
          </ol>
        </div>
      ) : null}

      {remoteError ? (
        <div role="alert" className="mt-4 flex items-center justify-between gap-4 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
          <span className="text-danger">读取牌组/笔记类型失败：{remoteError.message}</span>
          <Button variant="secondary" size="sm" onClick={() => void loadRemoteData()}>
            重试
          </Button>
        </div>
      ) : null}

      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="mt-8 flex flex-1 flex-col gap-6"
      >
        <Field
          label="AnkiConnect 地址"
          id="anki-endpoint"
          hint="默认 http://localhost:8765。"
          error={errors.endpoint ?? null}
        >
          <input
            value={draft.endpoint}
            onChange={(event) => {
              setDraft((current) => ({ ...current, endpoint: event.target.value }));
              setErrors((current) => ({ ...current, endpoint: undefined }));
              setSaved(false);
            }}
            className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            placeholder="http://localhost:8765"
          />
        </Field>

        <div className="grid grid-cols-2 gap-4">
          <Field
            label="牌组"
            id="anki-deck"
            hint={decks === null ? "连接 Anki 后可选择。" : undefined}
            error={errors.deck ?? null}
          >
            <select
              value={draft.deck}
              onChange={(event) => {
                setDraft((current) => ({ ...current, deck: event.target.value }));
                setErrors((current) => ({ ...current, deck: undefined }));
                setSaved(false);
              }}
              disabled={decks === null}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1 disabled:opacity-48"
            >
              {deckOptions.length === 0 ? <option value={draft.deck}>{draft.deck || "未连接"}</option> : null}
              {deckOptions.map((deck) => (
                <option key={deck} value={deck}>
                  {deck}
                </option>
              ))}
            </select>
          </Field>

          <Field
            label="笔记类型"
            id="anki-model"
            hint={models === null ? "连接 Anki 后可选择。" : undefined}
            error={errors.model ?? null}
          >
            <select
              value={draft.model}
              onChange={(event) => {
                setDraft((current) => ({ ...current, model: event.target.value }));
                setErrors((current) => ({ ...current, model: undefined }));
                setSaved(false);
              }}
              disabled={models === null}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1 disabled:opacity-48"
            >
              {modelOptions.length === 0 ? <option value={draft.model}>{draft.model || "未连接"}</option> : null}
              {modelOptions.map((model) => (
                <option key={model} value={model}>
                  {model}
                </option>
              ))}
            </select>
          </Field>
        </div>

        <fieldset className="flex flex-col gap-3">
          <legend className="text-sm font-medium">字段映射</legend>
          {loadingRemote ? <Spinner label="正在读取笔记类型字段…" /> : null}
          <div className="grid grid-cols-2 gap-4">
            {FIELD_LABELS.map(({ key, label }) => {
              const current = draft.field_map[key];
              const fieldOptions =
                availableFields.includes(current) || current.length === 0
                  ? availableFields
                  : [current, ...availableFields];
              return (
                <Field
                  key={key}
                  label={label}
                  id={`anki-field-${key}`}
                  error={errors.mapping[key] ?? null}
                >
                  <select
                    value={current}
                    onChange={(event) => updateFieldMap(key, event.target.value)}
                    disabled={availableFields.length === 0}
                    className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1 disabled:opacity-48"
                  >
                    {current.length === 0 ? <option value="">请选择字段</option> : null}
                    {fieldOptions.map((field) => (
                      <option key={field} value={field}>
                        {field}
                      </option>
                    ))}
                  </select>
                </Field>
              );
            })}
          </div>
          {availableFields.length === 0 ? (
            <p className="text-xs text-ink-muted">连接 Anki 并选择笔记类型后，这里会列出可映射字段。</p>
          ) : null}
        </fieldset>

        <Field label="标签" id="anki-tags" hint="用逗号分隔；导出时附加到每张卡片。">
          <input
            value={tagsText}
            onChange={(event) => {
              setTagsText(event.target.value);
              setSaved(false);
            }}
            className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            placeholder="global-dict"
          />
        </Field>

        <fieldset className="flex flex-col gap-3">
          <legend className="text-sm font-medium">同语境重复</legend>
          <div role="radiogroup" aria-label="同语境重复提示" className="flex flex-col gap-2">
            {[
              {
                value: "warn" as const,
                label: "提示后仍然保存",
                description: "同词同句同释义只提示一次，不阻断不同语境。",
              },
              {
                value: "allow" as const,
                label: "静默保存",
                description: "不显示同语境提示，直接写入。",
              },
            ].map((option) => {
              const checked = draft.duplicate_policy === option.value;
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
                    name="duplicate-policy"
                    value={option.value}
                    checked={checked}
                    onChange={() => {
                      setDraft((current) => ({ ...current, duplicate_policy: option.value }));
                      setSaved(false);
                    }}
                    className="size-4 accent-accent"
                  />
                </label>
              );
            })}
          </div>
          <p className="text-xs text-ink-muted">
            导出始终允许重复卡片；此设置只控制是否提示同词同句同释义。
          </p>
        </fieldset>

        {saveError ? (
          <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {saveError.code}
            </p>
            {saveError.code === "ANKI_FIELD_MISSING" ? (
              <p className="mt-1 text-xs text-ink-muted">请确认四个字段都存在于所选笔记类型。</p>
            ) : null}
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
