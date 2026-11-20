// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { BridgeError, createBridgeClient, type HealthResult } from "../../lib/bridge";
import {
  DEFAULT_SETTINGS,
  loadSettings,
  saveSettings,
  validatePort,
  validateToken,
} from "../../lib/settings";

type FieldErrors = { port?: string; token?: string };

function failureMessage(cause: unknown): string {
  if (cause instanceof BridgeError) {
    switch (cause.code) {
      case "AUTH_FAILED":
        return "令牌错误：从桌面端“设置 → 浏览器扩展”重新复制令牌，再点“测试连接”。";
      case "DESKTOP_OFFLINE":
        return "桌面端未连接：启动 LexiGrab，并在桌面端设置里确认桥接已启用。";
      case "TIMEOUT":
        return "请求超时：重试一次；桌面端可能正在加载词典。";
      case "NOT_CONFIGURED":
        return "还没有配对：填入桌面端的端口与令牌后重试。";
      default:
        return `${cause.message}（错误码：${cause.code}）`;
    }
  }
  return "连接失败：未知错误，请重新测试连接。";
}

/**
 * 扩展选项页（APP_FLOW §8.1）：端口、令牌、测试连接与保存。
 * 只写 `storage.local`；令牌默认遮罩，不打印到页面。
 */
export function App() {
  const [port, setPort] = useState<number>(DEFAULT_SETTINGS.port);
  const [token, setToken] = useState("");
  const [reveal, setReveal] = useState(false);
  const [loading, setLoading] = useState(true);
  const [testing, setTesting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [errors, setErrors] = useState<FieldErrors>({});
  const [notice, setNotice] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    loadSettings()
      .then((settings) => {
        if (!active) return;
        setPort(settings.port);
        setToken(settings.token);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const validate = (): FieldErrors => {
    const next: FieldErrors = {};
    const portError = validatePort(port);
    if (portError) next.port = portError;
    const tokenError = validateToken(token);
    if (tokenError) next.token = tokenError;
    return next;
  };

  const test = async () => {
    if (testing) return;
    const nextErrors = validate();
    setErrors(nextErrors);
    setFailure(null);
    setNotice(null);
    if (nextErrors.port || nextErrors.token) return;
    setTesting(true);
    try {
      const client = createBridgeClient({ port, token: token.trim() });
      const health: HealthResult = await client.health();
      setNotice(`连接成功：${health.app} ${health.version}（API v${health.api_version}）`);
    } catch (cause) {
      setFailure(failureMessage(cause));
    } finally {
      setTesting(false);
    }
  };

  const save = async () => {
    if (saving) return;
    const nextErrors = validate();
    setErrors(nextErrors);
    if (nextErrors.port || nextErrors.token) return;
    setSaving(true);
    setFailure(null);
    try {
      await saveSettings({ port, token: token.trim() });
      setNotice("已保存；回到页面划词即可查询。");
    } catch {
      setFailure("保存失败：请确认扩展有 storage 权限。");
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="mx-auto flex max-w-[520px] flex-col gap-6 px-6 py-10">
      <header>
        <h1 className="text-[24px] font-medium tracking-tight">LexiGrab 扩展设置</h1>
        <p className="mt-1 text-sm text-ink-muted">
          填写桌面端“设置 → 浏览器扩展”里显示的端口与配对令牌。扩展只访问 127.0.0.1。
        </p>
      </header>

      {loading ? (
        <p role="status" className="text-sm text-ink-muted">
          正在读取设置…
        </p>
      ) : (
        <form
          noValidate
          onSubmit={(event) => {
            event.preventDefault();
            void save();
          }}
          className="flex flex-col gap-5 rounded-2xl bg-canvas px-6 py-6 shadow-sm"
        >
          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium">端口</span>
            <input
              inputMode="numeric"
              value={port}
              onChange={(event) => {
                setPort(Number.parseInt(event.target.value, 10) || 0);
                setErrors((current) => ({ ...current, port: undefined }));
                setNotice(null);
              }}
              aria-invalid={errors.port ? true : undefined}
              aria-describedby={errors.port ? "port-error" : "port-hint"}
              className="h-10 rounded-lg border border-hairline bg-canvas px-3 text-sm outline-none focus:ring-2 focus:ring-accent"
            />
            {errors.port ? (
              <span id="port-error" role="alert" className="text-xs text-danger">
                {errors.port}
              </span>
            ) : (
              <span id="port-hint" className="text-xs text-ink-muted">
                默认 12787；与桌面端保持一致。
              </span>
            )}
          </label>

          <label className="flex flex-col gap-1.5 text-sm">
            <span className="font-medium">配对令牌</span>
            <span className="flex items-center gap-2">
              <input
                type={reveal ? "text" : "password"}
                value={token}
                onChange={(event) => {
                  setToken(event.target.value);
                  setErrors((current) => ({ ...current, token: undefined }));
                  setNotice(null);
                }}
                aria-invalid={errors.token ? true : undefined}
                aria-describedby={errors.token ? "token-error" : "token-hint"}
                autoComplete="off"
                spellCheck={false}
                className="h-10 min-w-0 flex-1 rounded-lg border border-hairline bg-canvas px-3 font-mono text-xs outline-none focus:ring-2 focus:ring-accent"
              />
              <button
                type="button"
                onClick={() => setReveal((current) => !current)}
                className="h-10 shrink-0 rounded-lg border border-hairline px-3 text-xs"
              >
                {reveal ? "隐藏" : "显示"}
              </button>
            </span>
            {errors.token ? (
              <span id="token-error" role="alert" className="text-xs text-danger">
                {errors.token}
              </span>
            ) : (
              <span id="token-hint" className="text-xs text-ink-muted">
                桌面端生成的 64 位十六进制令牌；令牌轮换后旧令牌立即失效，需要重新填写并测试连接。
              </span>
            )}
          </label>

          {notice ? (
            <p role="status" className="rounded-lg bg-canvas-soft px-3 py-2 text-xs text-success">
              {notice}
            </p>
          ) : null}
          {failure ? (
            <p role="alert" className="rounded-lg border border-danger/40 px-3 py-2 text-xs text-danger">
              {failure}
            </p>
          ) : null}

          <div className="flex items-center gap-2">
            <button
              type="submit"
              disabled={saving}
              className="h-9 rounded-full bg-ink px-5 text-xs font-medium text-on-accent disabled:opacity-48"
            >
              {saving ? "保存中…" : "保存"}
            </button>
            <button
              type="button"
              onClick={() => void test()}
              disabled={testing}
              className="h-9 rounded-full border border-hairline px-5 text-xs font-medium disabled:opacity-48"
            >
              {testing ? "测试中…" : "测试连接"}
            </button>
          </div>
        </form>
      )}

      <section className="rounded-2xl bg-canvas px-6 py-5 text-sm text-ink-muted shadow-sm">
        <h2 className="text-sm font-medium text-ink">配对三步</h2>
        <ol className="mt-2 flex list-decimal flex-col gap-1 pl-5">
          <li>启动 LexiGrab 桌面端，进入“设置 → 浏览器扩展”，确认桥接处于“运行中”。</li>
          <li>复制令牌，填入上面的端口与令牌，点击“测试连接”。</li>
          <li>看到“连接成功”后保存；回到任意页面划词，点浮动按钮查询。</li>
        </ol>
        <p className="mt-3 text-xs">
          未签名 .xpi 只能在 Firefox Developer Edition / Nightly 中安装；正式版可用
          about:debugging 临时加载。
        </p>
      </section>
    </main>
  );
}
