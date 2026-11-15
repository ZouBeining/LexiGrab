/**
 * 扩展设置存储。只写 `browser.storage.local`（不用 sync）：
 * 令牌属于本机配对信息，不应该随 Firefox 账号同步到其他设备。
 */
import { browser } from "wxt/browser";
import { DEFAULT_PORT } from "./bridge";

export interface ExtensionSettings {
  port: number;
  token: string;
}

export const DEFAULT_SETTINGS: ExtensionSettings = {
  port: DEFAULT_PORT,
  token: "",
};

const KEY = "lexigrab:settings";

interface StoredSettings {
  port?: unknown;
  token?: unknown;
}

/** 读取设置；缺失或类型不正确时回落到默认值。 */
export async function loadSettings(): Promise<ExtensionSettings> {
  let raw: unknown;
  try {
    const result = await browser.storage.local.get(KEY);
    raw = result[KEY];
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
  if (!raw || typeof raw !== "object") {
    return { ...DEFAULT_SETTINGS };
  }
  const stored = raw as StoredSettings;
  const port =
    typeof stored.port === "number" && Number.isInteger(stored.port) && stored.port >= 1024
      ? stored.port
      : DEFAULT_PORT;
  const token = typeof stored.token === "string" ? stored.token : "";
  return { port, token };
}

export async function saveSettings(settings: ExtensionSettings): Promise<void> {
  await browser.storage.local.set({ [KEY]: settings });
}

/** 保存前校验端口范围；返回错误文案或 null。 */
export function validatePort(port: number): string | null {
  if (!Number.isInteger(port) || port < 1024 || port > 65535) {
    return "端口需为 1024–65535 的整数。";
  }
  return null;
}

/** 令牌格式：桌面端生成 64 位十六进制。 */
export function validateToken(token: string): string | null {
  if (!/^[0-9a-fA-F]{64}$/.test(token.trim())) {
    return "令牌应为 64 位十六进制字符串，请从桌面端设置复制。";
  }
  return null;
}
