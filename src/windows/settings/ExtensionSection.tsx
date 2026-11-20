// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";
import { getBridgeStatus, getSettings, regenerateBridgeToken, toIpcError, updateSettings } from "../../lib/ipc";
import { onBridgeStatus } from "../../lib/events";
import type { BridgeSettings, BridgeStatus, IpcError, Settings } from "../../lib/types";
import { Button } from "../../components/Button";
import { Field } from "../../components/Field";

/** “已连接”的新鲜度窗口，与最近成功请求时间比较。 */
const CONNECTED_WINDOW_MS = 5 * 60 * 1000;

const PAIRING_STEPS = [
  "在本分区点击“复制令牌”（端口默认 12787，可按需修改并保存）。",
  "在 Firefox 打开扩展设置页，填入端口与令牌，点击“测试连接”。",
  "测试通过后保存；回到页面划词即可查询与制卡。",
];

function formatLastRequest(status: BridgeStatus | null): string {
  if (!status?.last_success_ms) return "还没有收到扩展请求";
  const age = Date.now() - status.last_success_ms;
  if (age < 60_000) return "刚刚";
  if (age < 3_600_000) return `${Math.floor(age / 60_000)} 分钟前`;
  return new Date(status.last_success_ms).toLocaleString();
}

function statusLabel(status: BridgeStatus | null): string {
  if (!status) return "未知";
  if (!status.enabled) return "未启用";
  if (status.running) return "运行中";
  return "启动失败";
}

/** 最近成功请求是否落在“已连接”窗口内。 */
function isConnected(status: BridgeStatus | null): boolean {
  if (!status || !status.running || status.last_success_ms === null) return false;
  return Date.now() - status.last_success_ms < CONNECTED_WINDOW_MS;
}

async function copyText(value: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(value);
    return true;
  } catch {
    return false;
  }
}

/** 保存失败的下一步动作；文案与扩展 401/离线/端口占用提示保持一致。 */
function saveErrorAction(code: string): string {
  switch (code) {
    case "BRIDGE_PORT_IN_USE":
      return "换一个端口后重试；桌面端不会自动改用其他端口。";
    case "CONFIG_INVALID":
      return "检查端口与启用开关后重试。";
    default:
      return "检查配置后重试；错误码可用于排查日志。";
  }
}

interface ExtensionSectionProps {
  settings: Settings;
  onSaved: (next: Settings) => void;
}

/**
 * 浏览器扩展设置（docs/APP_FLOW.md §7.3）：桥接开关、端口、状态、令牌与配对三步。
 * 令牌默认遮罩；轮换需要二次确认，确认后旧令牌立即失效。
 */
export function ExtensionSection({ settings, onSaved }: ExtensionSectionProps) {
  const [draft, setDraft] = useState<BridgeSettings>({ ...settings.bridge });
  const [revealToken, setRevealToken] = useState(false);
  const [status, setStatus] = useState<BridgeStatus | null>(null);
  const [statusError, setStatusError] = useState<IpcError | null>(null);
  const [portError, setPortError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<IpcError | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  const [confirmRotate, setConfirmRotate] = useState(false);
  const [rotating, setRotating] = useState(false);
  const [rotateError, setRotateError] = useState<IpcError | null>(null);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await getBridgeStatus());
      setStatusError(null);
    } catch (cause: unknown) {
      setStatusError(toIpcError(cause));
    }
  }, []);

  useEffect(() => {
    let active = true;
    getBridgeStatus()
      .then((next) => {
        if (active) setStatus(next);
      })
      .catch((cause: unknown) => {
        if (active) setStatusError(toIpcError(cause));
      });
    let unlisten: (() => void) | null = null;
    onBridgeStatus((next) => setStatus(next))
      .then((off) => {
        if (active) unlisten = off;
        else off();
      })
      .catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const update = <K extends keyof BridgeSettings>(key: K, value: BridgeSettings[K]) => {
    setDraft((current) => ({ ...current, [key]: value }));
    setSaved(false);
    setSaveError(null);
  };

  const validatePort = (): boolean => {
    const port = draft.port;
    if (!Number.isInteger(port) || port < 1024 || port > 65535) {
      setPortError("端口需为 1024–65535 的整数。");
      return false;
    }
    setPortError(null);
    return true;
  };

  const save = async () => {
    if (saving) return;
    if (!validatePort()) return;
    setSaving(true);
    setSaveError(null);
    try {
      const updated = await updateSettings({ ...settings, bridge: draft });
      onSaved(updated);
      setDraft({ ...updated.bridge });
      setSaved(true);
      await refreshStatus();
    } catch (cause: unknown) {
      setSaveError(toIpcError(cause));
    } finally {
      setSaving(false);
    }
  };

  const rotate = async () => {
    if (rotating) return;
    setRotating(true);
    setRotateError(null);
    try {
      await regenerateBridgeToken();
      const refreshed = await getSettings();
      onSaved(refreshed);
      setDraft({ ...refreshed.bridge });
      setConfirmRotate(false);
      setRevealToken(false);
      setSaved(false);
      await refreshStatus();
    } catch (cause: unknown) {
      setRotateError(toIpcError(cause));
    } finally {
      setRotating(false);
    }
  };

  const copyToken = async () => {
    const ok = await copyText(draft.token);
    setCopied(ok);
    if (ok) window.setTimeout(() => setCopied(false), 2000);
  };

  const dirty = JSON.stringify(draft) !== JSON.stringify(settings.bridge);
  const connected = isConnected(status);

  return (
    <section aria-labelledby="extension-title" className="flex min-h-full max-w-[560px] flex-col gap-6">
      <header>
        <h1 id="extension-title" className="text-[26px] font-medium tracking-tight">
          浏览器扩展
        </h1>
        <p className="mt-1 text-sm text-ink-muted">
          桌面端在 127.0.0.1 上提供本地桥接，Firefox 扩展用配对令牌访问；只监听回环地址。
        </p>
      </header>

      <div className="rounded-2xl bg-canvas-soft px-5 py-4">
        <div className="flex items-center justify-between gap-4">
          <div className="min-w-0">
            <p className="flex items-center gap-2 text-sm">
              <span
                aria-hidden="true"
                className={`size-2 rounded-full ${connected ? "bg-accent" : status?.running ? "bg-ink-muted" : "bg-danger"}`}
              />
              <span className="font-medium">{statusLabel(status)}</span>
              {connected ? <span className="text-xs text-ink-muted">扩展已连接</span> : null}
            </p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {status?.url ?? "http://127.0.0.1:12787"} · API v{status?.api_version ?? 1}
            </p>
            <p className="mt-1 text-xs text-ink-muted">最近请求：{formatLastRequest(status)}</p>
            {status?.error ? (
              <div role="alert" className="mt-2 text-xs">
                <p className="font-medium text-danger">桥接启动失败</p>
                <p className="mt-0.5 text-ink-muted">{status.error}</p>
                <p className="mt-0.5 text-ink-muted">
                  换一个端口后保存；端口占用时不会自动换端口。
                </p>
              </div>
            ) : null}
          </div>
          <Button variant="secondary" size="sm" onClick={() => void refreshStatus()}>
            刷新状态
          </Button>
        </div>
        {statusError ? (
          <p role="alert" className="mt-2 text-xs text-danger">
            读取桥接状态失败：{statusError.message}
            <span className="ml-2 text-ink-muted">点击“刷新状态”重试。</span>
          </p>
        ) : null}
      </div>

      <form
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
        className="flex flex-col gap-6"
      >
        <label className="flex items-start gap-3 text-sm">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(event) => update("enabled", event.target.checked)}
            className="mt-0.5 size-4 accent-accent"
          />
          <span>
            <span className="block font-medium">启用本地桥接</span>
            <span className="mt-0.5 block text-xs text-ink-muted">
              关闭后扩展立刻无法连接；已保存的端口与令牌保持不变。
            </span>
          </span>
        </label>

        <Field
          label="端口"
          id="bridge-port"
          hint="范围 1024–65535；端口占用时不会自动换端口。"
          error={portError}
        >
          <input
            inputMode="numeric"
            value={draft.port}
            onChange={(event) => {
              update("port", Number.parseInt(event.target.value, 10) || 0);
              setPortError(null);
            }}
            className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent focus:ring-offset-1"
          />
        </Field>

        <Field
          label="配对令牌"
          id="bridge-token"
          hint="扩展设置页需要填入同一令牌；不要分享到公开渠道。"
        >
          <div className="flex items-center gap-2">
            <input
              readOnly
              type={revealToken ? "text" : "password"}
              value={draft.token}
              aria-label="配对令牌"
              className="h-10 min-w-0 flex-1 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none"
            />
            <Button
              variant="secondary"
              size="sm"
              type="button"
              onClick={() => setRevealToken((current) => !current)}
            >
              {revealToken ? "隐藏" : "显示"}
            </Button>
            <Button variant="secondary" size="sm" type="button" onClick={() => void copyToken()}>
              {copied ? "已复制" : "复制"}
            </Button>
          </div>
          <p role="status" aria-live="polite" className="sr-only">
            {copied ? "令牌已复制" : ""}
          </p>
        </Field>

        <div className="rounded-2xl border border-hairline px-5 py-4">
          <p className="text-sm font-medium">轮换令牌</p>
          <p className="mt-1 text-xs text-ink-muted">
            轮换后旧令牌立即失效，扩展会提示“令牌错误”，需要重新配对。已保存的卡片不受影响。
          </p>
          {rotateError ? (
            <div role="alert" className="mt-2 text-xs">
              <p className="font-medium text-danger">轮换失败</p>
              <p className="mt-0.5 text-ink-muted">{rotateError.message}</p>
              <p className="mt-0.5 text-ink-muted">重试；如仍失败，重启桌面端后再轮换。</p>
              <p className="mt-0.5 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                {rotateError.code}
              </p>
            </div>
          ) : null}
          <div className="mt-3">
            {confirmRotate ? (
              <div className="flex items-center gap-2">
                <Button size="sm" onClick={() => void rotate()} disabled={rotating}>
                  {rotating ? "轮换中…" : "确认轮换"}
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  type="button"
                  onClick={() => setConfirmRotate(false)}
                  disabled={rotating}
                >
                  取消
                </Button>
              </div>
            ) : (
              <Button
                variant="secondary"
                size="sm"
                type="button"
                onClick={() => setConfirmRotate(true)}
              >
                重新生成令牌
              </Button>
            )}
          </div>
        </div>

        <div className="rounded-2xl bg-canvas-soft px-5 py-4">
          <p className="text-sm font-medium">配对三步</p>
          <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5 text-sm text-ink-muted">
            {PAIRING_STEPS.map((step) => (
              <li key={step}>{step}</li>
            ))}
          </ol>
          <p className="mt-2 text-xs text-ink-muted">
            扩展只访问 127.0.0.1；未签名 .xpi 需在 Firefox Developer Edition/Nightly 安装。
          </p>
        </div>

        {saveError ? (
          <div role="alert" className="rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">保存失败</p>
            <p className="mt-1 text-ink-muted">{saveError.message}</p>
            <p className="mt-1 text-ink-muted">{saveErrorAction(saveError.code)}</p>
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
