// SPDX-License-Identifier: AGPL-3.0-only
/**
 * content script ↔ background 消息协议。
 *
 * content 不直接 fetch：所有网络请求在 background 执行，令牌只留在 background
 * 的 storage.local 读取路径里。响应统一为 `{ ok, data } | { ok, error }`，
 * 因为 `runtime.sendMessage` 只传可序列化值，Error 实例会丢字段。
 */
import { browser } from "wxt/browser";
import {
  BridgeError,
  type AnkiPayload,
  type BridgeErrorCode,
  type SelectionPayload,
} from "./bridge";

export type RuntimeRequest =
  | { type: "lexigrab:health" }
  | { type: "lexigrab:selection"; payload: SelectionPayload }
  | { type: "lexigrab:anki"; payload: AnkiPayload }
  | { type: "lexigrab:sense"; queryId: string };

export interface BridgeFailure {
  code: BridgeErrorCode;
  message: string;
}

export type RuntimeResponse<T> = { ok: true; data: T } | { ok: false; error: BridgeFailure };

export function failureOf(cause: unknown): BridgeFailure {
  if (cause instanceof BridgeError) {
    return { code: cause.code, message: cause.message };
  }
  return { code: "INVALID_RESPONSE", message: "扩展内部错误，请重试" };
}

/** 发送请求并解包；失败时抛出带固定错误码的 BridgeError。 */
export async function sendRequest<T>(request: RuntimeRequest): Promise<T> {
  let response: RuntimeResponse<T> | undefined;
  try {
    response = (await browser.runtime.sendMessage(request)) as RuntimeResponse<T> | undefined;
  } catch {
    throw new BridgeError("DESKTOP_OFFLINE", "后台脚本不可用，请重新加载扩展");
  }
  if (!response || typeof response !== "object" || !("ok" in response)) {
    throw new BridgeError("INVALID_RESPONSE", "后台脚本返回了无法解析的响应");
  }
  if (!response.ok) {
    throw new BridgeError(response.error.code, response.error.message);
  }
  return response.data;
}
