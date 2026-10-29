import { useCallback, useEffect, useState } from "react";
import { getRuntimeStatus, openConfigDir, openWindow, toIpcError } from "../../lib/ipc";
import type { IpcError, RuntimeStatus } from "../../lib/types";
import { Button } from "../../components/Button";
import { Spinner } from "../../components/Spinner";
import { clearGuideDismissed } from "../manager/FirstRunGuide";

/** Key read by the manager window to force the first-run guide open. */
const FORCE_GUIDE_KEY = "lexigrab.guide.force";

/** About section (docs/APP_FLOW.md §7.6): version, directories, license and guide reset. */
export function AboutSection() {
  const [status, setStatus] = useState<RuntimeStatus | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);

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

  const retry = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);

  const openConfig = useCallback(async () => {
    try {
      await openConfigDir();
    } catch (cause: unknown) {
      setError(toIpcError(cause));
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
        <p className="mt-1 text-sm text-ink-muted">版本、目录、许可证与首次引导入口。</p>
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
          <dl className="rounded-3xl border border-hairline">
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">版本</dt>
              <dd className="font-mono text-xs">{status.version}</dd>
            </div>
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">配置目录</dt>
              <dd className="min-w-0 max-w-[340px] truncate font-mono text-xs" title={status.config_dir}>
                {status.config_dir}
              </dd>
            </div>
            <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4">
              <dt className="text-sm text-ink-muted">日志目录</dt>
              <dd className="min-w-0 max-w-[340px] truncate font-mono text-xs" title={status.log_dir}>
                {status.log_dir}
              </dd>
            </div>
            <div className="flex items-center justify-between gap-6 px-6 py-4">
              <dt className="text-sm text-ink-muted">缓存文件</dt>
              <dd className="min-w-0 max-w-[340px] truncate font-mono text-xs" title={status.cache_path}>
                {status.cache_path}
              </dd>
            </div>
          </dl>
          <div className="mt-4 flex flex-wrap gap-3">
            <Button variant="secondary" size="sm" onClick={() => void openConfig()}>
              打开配置目录
            </Button>
            <Button variant="secondary" size="sm" onClick={reopenGuide}>
              重新打开首次引导
            </Button>
          </div>
        </div>
      ) : null}

      <div className="rounded-3xl bg-block-1 px-6 py-6">
        <p className="font-mono text-[11px] tracking-[0.6px] text-ink-muted">许可证</p>
        <p className="mt-2 text-sm">
          LexiGrab 以 AGPL-3.0 发布。第三方依赖与许可证清单见仓库的 LICENSE 与相关文档。
        </p>
        <p className="mt-2 text-xs text-ink-muted">
          本地词典与缓存都留在本机；应用不包含遥测。
        </p>
      </div>
    </section>
  );
}
