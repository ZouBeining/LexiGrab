// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";
import type { ReactNode } from "react";
import { getRuntimeStatus, openConfigDir, openWindow, toIpcError } from "../../lib/ipc";
import type { IpcError, RuntimeStatus } from "../../lib/types";
import { Button } from "../../components/Button";
import { Spinner } from "../../components/Spinner";
import { CaptureTestPanel } from "./CaptureTestPanel";

interface RowProps {
  label: string;
  children: ReactNode;
}

function Row({ label, children }: RowProps) {
  return (
    <div className="flex items-center justify-between gap-6 border-b border-hairline px-6 py-4 last:border-b-0">
      <dt className="text-sm text-ink-muted">{label}</dt>
      <dd className="min-w-0 text-right">{children}</dd>
    </div>
  );
}

/**
 * Runtime status view (docs/APP_FLOW.md §2.1): hotkey, version, directories,
 * plus the M0 capture probe kept as a diagnostic tool for M2.
 */
export function StatusView() {
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

  const refresh = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);

  const openConfig = async () => {
    try {
      await openConfigDir();
    } catch (cause: unknown) {
      setError(toIpcError(cause));
    }
  };

  return (
    <section aria-labelledby="status-title" className="flex flex-col gap-8">
      <header className="flex items-start justify-between gap-4">
        <div>
          <h1 id="status-title" className="text-[26px] font-medium tracking-tight">
            运行状态
          </h1>
          <p className="mt-1 text-sm text-ink-muted">热键、词典加载数与本地目录。</p>
        </div>
        <Button variant="secondary" onClick={refresh}>
          刷新
        </Button>
      </header>

      {loading ? (
        <div className="flex items-center gap-2 border-y border-hairline py-8">
          <Spinner label="正在读取运行状态…" />
        </div>
      ) : error ? (
        <div role="alert" className="rounded-3xl border border-danger/40 px-6 py-8">
          <p className="text-sm font-medium text-danger">读取运行状态失败</p>
          <p className="mt-1 text-sm text-ink-muted">{error.message}</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{error.code}</p>
          <Button variant="secondary" size="sm" className="mt-3" onClick={refresh}>
            重试
          </Button>
        </div>
      ) : status ? (
        <div>
          <dl className="rounded-3xl border border-hairline">
            <Row label="版本">
              <span className="font-mono text-xs">{status.version}</span>
            </Row>
            <Row label="全局热键">
              <span className="inline-flex items-center gap-3">
                <span className="rounded-full border border-hairline px-2 py-1 font-mono text-xs">
                  {status.hotkey.accelerator}
                </span>
                <span className="inline-flex items-center gap-2 text-xs text-ink-muted">
                  <span
                    aria-hidden="true"
                    className={`size-1.5 rounded-full ${status.hotkey.registered ? "bg-success" : "bg-danger"}`}
                  />
                  {status.hotkey.registered ? "已注册" : "未注册"}
                </span>
              </span>
            </Row>
            {status.hotkey.message ? (
              <div className="border-b border-hairline px-6 py-4">
                <p role="alert" className="text-sm text-danger">
                  {status.hotkey.message}
                </p>
              </div>
            ) : null}
            <Row label="已就绪词典源">
              <span className="font-mono text-xs">{status.sources_ready}</span>
            </Row>
            <Row label="配置目录">
              <span className="block max-w-[420px] truncate font-mono text-xs" title={status.config_dir}>
                {status.config_dir}
              </span>
            </Row>
            <Row label="日志目录">
              <span className="block max-w-[420px] truncate font-mono text-xs" title={status.log_dir}>
                {status.log_dir}
              </span>
            </Row>
            <Row label="缓存文件">
              <span className="block max-w-[420px] truncate font-mono text-xs" title={status.cache_path}>
                {status.cache_path}
              </span>
            </Row>
          </dl>
          <div className="mt-4 flex flex-wrap gap-3">
            <Button variant="secondary" size="sm" onClick={() => void openConfig()}>
              打开配置目录
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => {
                void openWindow("settings");
              }}
            >
              打开设置
            </Button>
          </div>
        </div>
      ) : null}

      <CaptureTestPanel />
    </section>
  );
}
