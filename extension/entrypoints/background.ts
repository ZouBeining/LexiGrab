// SPDX-License-Identifier: AGPL-3.0-only
/**
 * background：唯一持有桥接令牌的内存路径。
 * content 只通过消息请求，页面脚本拿不到令牌，也不会把完整响应写进 console。
 */
import { browser } from "wxt/browser";
import { failureOf, type RuntimeRequest, type RuntimeResponse } from "../lib/messages";
import { createBridgeClient } from "../lib/bridge";
import { loadSettings } from "../lib/settings";

export default defineBackground(() => {
  browser.runtime.onMessage.addListener((message, _sender, sendResponse) => {
    if (!isRuntimeRequest(message)) {
      return false;
    }
    void handle(message).then(sendResponse);
    return true;
  });

  browser.runtime.onInstalled.addListener((details) => {
    if (details.reason === "install") {
      void browser.runtime.openOptionsPage();
    }
  });
});

function isRuntimeRequest(value: unknown): value is RuntimeRequest {
  if (!value || typeof value !== "object") return false;
  const type = (value as { type?: unknown }).type;
  return (
    type === "lexigrab:health" ||
    type === "lexigrab:selection" ||
    type === "lexigrab:anki" ||
    type === "lexigrab:sense"
  );
}

async function handle(request: RuntimeRequest): Promise<RuntimeResponse<unknown>> {
  try {
    const settings = await loadSettings();
    const client = createBridgeClient({ port: settings.port, token: settings.token });
    switch (request.type) {
      case "lexigrab:health":
        return { ok: true, data: await client.health() };
      case "lexigrab:selection":
        return { ok: true, data: await client.selection(request.payload) };
      case "lexigrab:anki":
        return { ok: true, data: await client.anki(request.payload) };
      case "lexigrab:sense":
        return { ok: true, data: await client.sense(request.queryId) };
    }
  } catch (cause) {
    return { ok: false, error: failureOf(cause) };
  }
}
