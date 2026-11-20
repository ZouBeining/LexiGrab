// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";
import { getTauriVersion, getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  getRuntimeStatus,
  openCacheDir,
  openConfigDir,
  openLogDir,
  openWindow,
  toIpcError,
} from "../../lib/ipc";
import type { IpcError, RuntimeStatus } from "../../lib/types";
import { Button } from "../../components/Button";
import { Spinner } from "../../components/Spinner";
import { clearGuideDismissed } from "../manager/FirstRunGuide";

/** Key read by the manager window to force the first-run guide open. */
const FORCE_GUIDE_KEY = "lexigrab.guide.force";

const REPOSITORY_URL = "https://github.com/ZouBeining/LexiGrab";
const LICENSE_URL = "https://github.com/ZouBeining/LexiGrab/blob/main/LICENSE";

interface BuildInfo {
  /** `getVersion()`；测试环境不支持时回退到 `RuntimeStatus.version`。 */
  appVersion: string;
  /** `getTauriVersion()`；不可用时显示“未知”。 */
  tauriVersion: string | null;
  /** WebView2 的 Chromium 版本，从 user agent 读取。 */
  webviewVersion: string | null;
}

/** WebView2 在 UA 里以 `Edg/<version>` 出现；其他 Chromium WebView 回退 `Chrome/<version>`。 */
export function webviewVersionFrom(userAgent: string): string | null {
  const edge = /Edg\/([\d.]+)/.exec(userAgent);
  if (edge?.[1]) return edge[1];
  const chrome = /Chrome\/([\d.]+)/.exec(userAgent);
  return chrome?.[1] ?? null;
}

/** `RuntimeStatus.cache_path` 是 `cache.db` 文件；设置页展示并打开其父目录。 */
export function parentDirectory(path: string): string {
  const index = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return index > 0 ? path.slice(0, index) : path;
}

interface CopyNotice {
  tone: "info" | "error";
  text: string;
}

interface DirectoryRowProps {
  label: string;
  path: string;
  /** 目录行之间的分隔边框由调用方按位置补。 */
  className?: string;
  onCopy: () => void;
  onOpen: () => void;
}

function DirectoryRow({ label, path, className, onCopy, onOpen }: DirectoryRowProps) {
  return (
    <div
      className={["flex items-center justify-between gap-4 px-6 py-4", className]
        .filter(Boolean)
        .join(" ")}
    >
      <dt className="shrink-0 text-sm text-ink-muted">{label}</dt>
      <dd className="flex min-w-0 items-center gap-2">
        <span className="min-w-0 max-w-[240px] truncate font-mono text-xs" title={path}>
          {path}
        </span>
        <Button
          variant="secondary"
          size="sm"
          className="shrink-0"
          aria-label={`复制${label}路径`}
          onClick={onCopy}
        >
          复制路径
        </Button>
        <Button
          variant="secondary"
          size="sm"
          className="shrink-0"
          aria-label={`打开${label}`}
          onClick={onOpen}
        >
          打开
        </Button>
      </dd>
    </div>
  );
}

/** About section (docs/APP_FLOW.md §7.6): build info, directories, license and guide reset. */
export function AboutSection() {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);
  const [copyNotice, setCopyNotice] = useState<CopyNotice | null>(null);
  const [openError, setOpenError] = useState<IpcError | null>(null);
  const [linkError, setLinkError] = useState<IpcError | null>(null);

  /** UA 解析是纯读取；构建信息尚未返回时也能显示 WebView 版本。 */
  const webviewVersion = webviewVersionFrom(navigator.userAgent);
  const cacheDir = status ? parentDirectory(status.cache_path) : "";

  useEffect(() => {
    let active = true;
    getRuntimeStatus()
      .then((value) => {
        if (active) setStatus(value);
      })
      .catch((cause: unknown) => {
        if (active) setError(toIpcError(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  // Runtime metadata is best-effort: the webview version comes from the UA and a
  // missing Tauri API only falls back to "未知" (or RuntimeStatus.version).
  useEffect(() => {
    if (!status) return;
    let active = true;
    const webviewVersion = webviewVersionFrom(navigator.userAgent);
    Promise.allSettled([getVersion(), getTauriVersion()]).then(([app, tauri]) => {
      if (!active) return;
      setBuild({
        appVersion: app.status === "fulfilled" ? app.value : status.version,
        tauriVersion: tauri.status === "fulfilled" ? tauri.value : null,
        webviewVersion,
      });
    });
    return () => {
      active = false;
    };
  }, [status]);

  const retry = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);

  const openDirectory = useCallback(async (open: () => Promise<void>) => {
    setOpenError(null);
    try {
      await open();
    } catch (cause: unknown) {
      setOpenError(toIpcError(cause));
    }
  }, []);

  const copyPath = useCallback(async (path: string, label: string) => {
    setCopyNotice(null);
    try {
      await navigator.clipboard.writeText(path);
      setCopyNotice({ tone: "info", text: `已复制${label}路径。` });
    } catch {
      setCopyNotice({
        tone: "error",
        text: `复制${label}路径失败，请点击“打开”后在文件管理器里复制。`,
      });
    }
  }, []);

  const openExternal = useCallback(async (url: string) => {
    setLinkError(null);
    try {
      await openUrl(url);
    } catch {
      setLinkError({
        code: "INTERNAL",
        message: "无法打开链接，请复制页面上的仓库地址到浏览器打开。",
        detail: null,
      });
    }
  }, []);

  const reopenGuide = useCallback(() => {
    try {
      window.localStorage.setItem(FORCE_GUIDE_KEY, "1");
    } catch {
      // Ignore storage failures; the window still opens on the sources view.
    }
    clearGuideDismissed();
    void openWindow("manager");
  }, []);

  return (
    <section aria-labelledby="about-title" className="flex max-w-[560px] flex-col gap-8">
      <header>
        <h1 id="about-title" className="text-[26px] font-medium tracking-tight">
          关于
        </h1>
        <p className="mt-1 text-sm text-ink-muted">构建信息、目录、许可证与首次引导入口。</p>
      </header>

      {loading ? (
        <div className="flex items-center gap-2 border-y border-hairline py-8">
          <Spinner label="正在读取版本信息…" />
        </div>
      ) : error ? (
        <div role="alert" className="rounded-3xl border border-danger/40 px-6 py-6">
          <p className="text-sm font-medium text-danger">读取版本信息失败</p>
          <p className="mt-1 text-sm text-ink-muted">{error.message}</p>
          <Button variant="secondary" size="sm" className="mt-3" onClick={retry}>
            重试
          </Button>
        </div>
      ) : status ? (
        <div>
          <h2 className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">构建信息</h2>
          <dl className="mt-2 rounded-3xl border border-hairline">
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">应用版本</dt>
              <dd className="font-mono text-xs">{build?.appVersion ?? status.version}</dd>
            </div>
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">Tauri 运行时</dt>
              <dd className="font-mono text-xs">{build?.tauriVersion ?? "未知"}</dd>
            </div>
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">WebView 运行时</dt>
              <dd className="font-mono text-xs">{build?.webviewVersion ?? webviewVersion ?? "未知"}</dd>
            </div>
            <DirectoryRow
              label="配置目录"
              path={status.config_dir}
              className="border-b border-hairline"
              onCopy={() => void copyPath(status.config_dir, "配置目录")}
              onOpen={() => void openDirectory(openConfigDir)}
            />
            <DirectoryRow
              label="日志目录"
              path={status.log_dir}
              className="border-b border-hairline"
              onCopy={() => void copyPath(status.log_dir, "日志目录")}
              onOpen={() => void openDirectory(openLogDir)}
            />
            <DirectoryRow
              label="缓存目录"
              path={cacheDir}
              onCopy={() => void copyPath(cacheDir, "缓存目录")}
              onOpen={() => void openDirectory(openCacheDir)}
            />
          </dl>

          <div className="mt-4 flex flex-wrap gap-3">
            <Button variant="secondary" size="sm" onClick={reopenGuide}>
              重新打开首次引导
            </Button>
          </div>

          {copyNotice ? (
            <p
              role="status"
              aria-live="polite"
              className={`mt-3 text-xs ${
                copyNotice.tone === "error" ? "text-danger" : "text-ink-muted"
              }`}
            >
              {copyNotice.text}
            </p>
          ) : null}

          {openError ? (
            <div role="alert" className="mt-3 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
              <p className="font-medium text-danger">无法打开目录</p>
              <p className="mt-1 text-ink-muted">{openError.message}</p>
              <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
                {openError.code}
              </p>
            </div>
          ) : null}
        </div>
      ) : null}

      <div className="rounded-3xl bg-block-1 px-6 py-6">
        <p className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">许可证</p>
        <p className="mt-2 text-sm">
          LexiGrab 以 AGPL-3.0 发布。第三方依赖与许可证清单见仓库的 LICENSE 与相关文档。
        </p>
        <p className="mt-2 break-all font-mono text-xs text-ink-muted">{REPOSITORY_URL}</p>
        <div className="mt-3 flex flex-wrap gap-3">
          <Button variant="secondary" size="sm" onClick={() => void openExternal(LICENSE_URL)}>
            打开许可证
          </Button>
          <Button variant="secondary" size="sm" onClick={() => void openExternal(REPOSITORY_URL)}>
            打开仓库
          </Button>
        </div>
        {linkError ? (
          <div role="alert" className="mt-3 rounded-2xl border border-danger/40 px-4 py-3 text-sm">
            <p className="font-medium text-danger">无法打开链接</p>
            <p className="mt-1 text-ink-muted">{linkError.message}</p>
            <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">
              {linkError.code}
            </p>
          </div>
        ) : null}
        <p className="mt-2 text-xs text-ink-muted">
          本地词典与缓存都留在本机；应用不包含遥测。
        </p>
      </div>
    </section>
  );
}
