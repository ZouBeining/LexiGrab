import { useCallback, useEffect, useState } from "react";
import {
  clearLookupCache,
  clearSenseCache,
  getCacheStats,
  toIpcError,
  updateSettings,
} from "../../lib/ipc";
import type { CacheSettings, CacheStats, IpcError, Settings } from "../../lib/types";
import { Button } from "../../components/Button";
import { EmptyState } from "../../components/EmptyState";
import { Field } from "../../components/Field";
import { Spinner } from "../../components/Spinner";

type ClearTarget = "lookup" | "sense";

interface FieldErrors {
  ttl?: string;
  negativeTtl?: string;
}

/** Human-readable byte size; the backend reports `entries_json` bytes. */
function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = unit === 0 ? String(Math.round(value)) : value.toFixed(1);
  return `${text} ${units[unit] ?? "B"}`;
}

interface CacheSectionProps {
  settings: Settings;
  onSaved: (next: Settings) => void;
}

/**
 * 缓存设置（APP_FLOW §7.4, PLAN_M4 T4.6）：TTL、统计、刷新与两类清理。
 * 清理词典缓存不删除 `saved_cards` 历史与语境缓存；两者分别二次确认。
 */
export function CacheSection({ settings, onSaved }: CacheSectionProps) {
  const [draft, setDraft] = useState<CacheSettings>({ ...settings.cache });
  const [errors, setErrors] = useState<FieldErrors>({});
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);

  const [stats, setStats] = useState<CacheStats | null>(null);
  const [statsError, setStatsError] = useState<IpcError | null>(null);
  const [statsLoading, setStatsLoading] = useState(true);

  const [confirm, setConfirm] = useState<ClearTarget | null>(null);
  const [clearing, setClearing] = useState(false);
  const [clearError, setClearError] = useState<IpcError | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const refreshStats = useCallback(async () => {
    setStatsLoading(true);
    setStatsError(null);
    try {
      setStats(await getCacheStats());
    } catch (cause: unknown) {
      setStatsError(toIpcError(cause));
    } finally {
      setStatsLoading(false);
    }
  }, []);

  useEffect(() => {
    let active = true;
    getCacheStats()
      .then((value) => {
        if (!active) return;
        setStats(value);
        setStatsError(null);
      })
      .catch((cause: unknown) => {
        if (active) setStatsError(toIpcError(cause));
      })
      .finally(() => {
        if (active) setStatsLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const update = <K extends keyof CacheSettings>(key: K, value: CacheSettings[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setSaved(false);
    setSaveError(null);
  };

  const validate = (): FieldErrors => {
    const next: FieldErrors = {};
    if (!Number.isInteger(draft.ttl_hours) || draft.ttl_hours < 1 || draft.ttl_hours > 720) {
      next.ttl = "缓存时长需为 1–720 小时的整数。";
    }
    if (
      !Number.isInteger(draft.negative_ttl_minutes) ||
      draft.negative_ttl_minutes < 1 ||
      draft.negative_ttl_minutes > 60
    ) {
      next.negativeTtl = "负缓存时长需为 1–60 分钟的整数。";
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
      const updated = await updateSettings({ ...settings, cache: draft });
      onSaved(updated);
      setDraft({ ...updated.cache });
      setSaved(true);
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  const runClear = async () => {
    if (!confirm || clearing) return;
    const target = confirm;
    setClearing(true);
    setClearError(null);
    setNotice(null);
    try {
      const result = target === "lookup" ? await clearLookupCache() : await clearSenseCache();
      setNotice(
        target === "lookup"
          ? `已清除 ${result.removed} 条词典缓存。`
          : `已清除 ${result.removed} 条语境缓存。`,
      );
      setConfirm(null);
      await refreshStats();
    } catch (cause: unknown) {
      setClearError(toIpcError(cause));
    } finally {
      setClearing(false);
    }
  };

  const dirty = JSON.stringify(draft) !== JSON.stringify(settings.cache);
  const cacheEmpty = stats !== null && stats.entries === 0 && stats.sense_entries === 0;

  return (
    <section aria-labelledby="cache-title" className="flex max-w-[560px] flex-col gap-8">
      <header>
        <h1 id="cache-title" className="text-[26px] font-medium tracking-tight">
          缓存
        </h1>
        <p className="mt-1 text-sm text-ink-muted">
          查询结果按 TTL 缓存；命中缓存时不再请求远程源。缓存与已保存的卡片分开管理。
        </p>
      </header>

      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="flex flex-col gap-6"
      >
        <div className="grid grid-cols-2 gap-4">
          <Field
            label="缓存时长（小时）"
            id="cache-ttl-hours"
            hint="范围 1–720；只影响新写入，旧条目按原过期时间。"
            error={errors.ttl ?? null}
          >
            <input
              inputMode="numeric"
              value={draft.ttl_hours}
              onChange={(event) => {
                update("ttl_hours", Number.parseInt(event.target.value, 10) || 0);
                setErrors((current) => ({ ...current, ttl: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            />
          </Field>
          <Field
            label="负缓存时长（分钟）"
            id="cache-negative-ttl"
            hint="范围 1–60；空结果在此期间不再请求。"
            error={errors.negativeTtl ?? null}
          >
            <input
              inputMode="numeric"
              value={draft.negative_ttl_minutes}
              onChange={(event) => {
                update("negative_ttl_minutes", Number.parseInt(event.target.value, 10) || 0);
                setErrors((current) => ({ ...current, negativeTtl: undefined }));
              }}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
            />
          </Field>
        </div>

        {saveError ? (
          <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {saveError.code}
            </p>
          </div>
        ) : null}

        <footer className="flex items-center justify-between gap-4 border-t border-hairline pt-4">
          <p role="status" aria-live="polite" className="text-xs text-ink-muted">
            {saved ? "已保存" : dirty ? "有未保存的更改" : ""}
          </p>
          <Button type="submit" disabled={saving}>
            {saving ? "保存中…" : "保存"}
          </Button>
        </footer>
      </form>

      <section aria-labelledby="cache-stats-title" className="rounded-3xl border border-hairline px-6 py-5">
        <div className="flex items-center justify-between gap-4">
          <div>
            <h2 id="cache-stats-title" className="text-[16px] font-medium">
              缓存统计
            </h2>
            <p className="mt-1 text-xs text-ink-muted">
              条目数、正/负缓存、语境条数与占用字节。
            </p>
          </div>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void refreshStats()}
            disabled={statsLoading}
          >
            刷新统计
          </Button>
        </div>

        <div className="mt-4">
          {statsLoading && stats === null ? (
            <div className="flex items-center gap-2 border-y border-hairline py-6">
              <Spinner label="正在读取缓存统计…" />
            </div>
          ) : statsError ? (
            <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
              <p className="font-medium text-danger">无法读取缓存统计</p>
              <p className="mt-1 text-ink-muted">{statsError.message}</p>
              <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                {statsError.code}
              </p>
              <Button
                variant="secondary"
                size="sm"
                className="mt-3"
                onClick={() => void refreshStats()}
              >
                重试
              </Button>
            </div>
          ) : stats ? (
            cacheEmpty ? (
              <EmptyState
                title="缓存为空"
                description="查询词典后会写入缓存；当前没有可清理的条目。"
                tone="plain"
              />
            ) : (
              <dl className="grid grid-cols-2 gap-x-6 gap-y-3 text-sm">
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">词典缓存</dt>
                  <dd className="font-mono text-xs">{stats.entries} 条</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">正缓存</dt>
                  <dd className="font-mono text-xs">{stats.positive_entries} 条</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">负缓存</dt>
                  <dd className="font-mono text-xs">{stats.empty_entries} 条</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">语境缓存</dt>
                  <dd className="font-mono text-xs">{stats.sense_entries} 条</dd>
                </div>
                <div className="flex items-center justify-between gap-3">
                  <dt className="text-ink-muted">占用</dt>
                  <dd className="font-mono text-xs">{formatBytes(stats.bytes)}</dd>
                </div>
              </dl>
            )
          ) : null}
        </div>
      </section>

      <section aria-labelledby="cache-clear-title" className="rounded-3xl border border-hairline px-6 py-5">
        <h2 id="cache-clear-title" className="text-[16px] font-medium">
          清除缓存
        </h2>
        <p className="mt-1 text-xs text-ink-muted">
          清除词典缓存不会删除已保存卡片的历史记录（saved_cards），也不会清除语境缓存；两类缓存分别清理。
        </p>

        {notice ? (
          <p role="status" className="mt-3 rounded-xl bg-canvas-soft px-3 py-2 text-xs text-ink-muted">
            {notice}
          </p>
        ) : null}

        {clearError ? (
          <div role="alert" className="mt-3 rounded-xl border border-danger/40 px-3 py-2 text-xs">
            <p className="text-danger">清理失败：{clearError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {clearError.code}
            </p>
          </div>
        ) : null}

        <div className="mt-4 flex flex-col gap-4">
          <div className="flex flex-col gap-2 rounded-xl bg-canvas-soft px-4 py-3">
            <p className="text-sm font-medium">词典缓存</p>
            <p className="text-xs text-ink-muted">
              只清除查询结果缓存；已保存的卡片历史与语境缓存保留。
            </p>
            {confirm === "lookup" ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" onClick={() => void runClear()} disabled={clearing}>
                  {clearing ? "清理中…" : "确认清除词典缓存"}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setConfirm(null)}
                  disabled={clearing}
                >
                  取消
                </Button>
              </div>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                className="self-start"
                onClick={() => {
                  setClearError(null);
                  setNotice(null);
                  setConfirm("lookup");
                }}
              >
                清除词典缓存
              </Button>
            )}
          </div>

          <div className="flex flex-col gap-2 rounded-xl bg-canvas-soft px-4 py-3">
            <p className="text-sm font-medium">语境缓存</p>
            <p className="text-xs text-ink-muted">
              清除选择器推荐与 AI 释义缓存；下次查询会重新请求。
            </p>
            {confirm === "sense" ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" onClick={() => void runClear()} disabled={clearing}>
                  {clearing ? "清理中…" : "确认清除语境缓存"}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => setConfirm(null)}
                  disabled={clearing}
                >
                  取消
                </Button>
              </div>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                className="self-start"
                onClick={() => {
                  setClearError(null);
                  setNotice(null);
                  setConfirm("sense");
                }}
              >
                清除语境缓存
              </Button>
            )}
          </div>
        </div>
      </section>
    </section>
  );
}
