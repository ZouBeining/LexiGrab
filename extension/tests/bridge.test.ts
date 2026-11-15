import { describe, expect, it, vi } from "vitest";
import {
  BridgeError,
  bridgeBaseUrl,
  createBridgeClient,
  type BridgeClientOptions,
} from "../lib/bridge";

const TOKEN = "a".repeat(64);

interface Call {
  url: string;
  init: RequestInit | undefined;
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function clientWith(
  responder: (call: Call) => Response | Promise<Response>,
  overrides: Partial<BridgeClientOptions> = {},
): { client: ReturnType<typeof createBridgeClient>; calls: Call[] } {
  const calls: Call[] = [];
  const fetchImpl = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const call = { url: String(input), init };
    calls.push(call);
    return responder(call);
  }) as unknown as typeof fetch;
  const client = createBridgeClient({
    port: 12787,
    token: TOKEN,
    fetchImpl,
    ...overrides,
  });
  return { client, calls };
}

describe("createBridgeClient", () => {
  it("没有令牌时返回 NOT_CONFIGURED 且不发请求", async () => {
    const { client, calls } = clientWith(() => jsonResponse({ ok: true }), { token: "  " });
    await expect(client.health()).rejects.toMatchObject({ code: "NOT_CONFIGURED" });
    expect(calls).toHaveLength(0);
  });

  it("请求打到 127.0.0.1 并带 X-Auth-Token", async () => {
    const { client, calls } = clientWith(() =>
      jsonResponse({ ok: true, app: "LexiGrab", version: "0.1.0", api_version: 1 }),
    );
    const health = await client.health();
    expect(health.api_version).toBe(1);
    expect(calls[0]?.url).toBe(`${bridgeBaseUrl(12787)}/v1/health`);
    const headers = calls[0]?.init?.headers as Record<string, string>;
    expect(headers["x-auth-token"]).toBe(TOKEN);
  });

  it.each([
    [401, "AUTH_FAILED"],
    [403, "ORIGIN_REJECTED"],
  ])("HTTP %s 映射为 %s", async (status, code) => {
    const { client } = clientWith(() => jsonResponse({ code: "X", message: "denied" }, status));
    await expect(client.health()).rejects.toMatchObject({ code, status });
  });

  it("网络错误映射为 DESKTOP_OFFLINE", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new TypeError("failed to fetch");
    }) as unknown as typeof fetch;
    const client = createBridgeClient({ port: 12787, token: TOKEN, fetchImpl });
    await expect(client.health()).rejects.toMatchObject({ code: "DESKTOP_OFFLINE" });
  });

  it("超时映射为 TIMEOUT", async () => {
    const fetchImpl = vi.fn(
      (_input: RequestInfo | URL, init?: RequestInit) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener("abort", () => {
            reject(new DOMException("aborted", "AbortError"));
          });
        }),
    ) as unknown as typeof fetch;
    const client = createBridgeClient({ port: 12787, token: TOKEN, fetchImpl, timeoutMs: 10 });
    await expect(client.health()).rejects.toMatchObject({ code: "TIMEOUT" });
  });

  it("服务端错误带上正文 message", async () => {
    const { client } = clientWith(() =>
      jsonResponse({ code: "INTERNAL", message: "桌面端内部错误" }, 500),
    );
    const error = await client.health().catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(BridgeError);
    expect(error).toMatchObject({ code: "SERVER_ERROR", message: "桌面端内部错误", status: 500 });
  });

  it("非法 JSON 映射为 INVALID_RESPONSE", async () => {
    const { client } = clientWith(() => new Response("not json", { status: 200 }));
    await expect(client.health()).rejects.toMatchObject({ code: "INVALID_RESPONSE" });
  });

  it("selection 返回契约字段", async () => {
    const { client } = clientWith(() =>
      jsonResponse({
        query_id: "q1",
        entries: [
          { headword: "hello", definition_html: "<p>hi</p>", phonetic: null, source_name: "Mini" },
        ],
        source_names: ["Mini"],
        partial: false,
        sense_available: true,
        elapsed_ms: 12,
      }),
    );
    const result = await client.selection({
      word: "hello",
      sentence: "Hello there.",
      url: "https://e.test",
      title: "T",
      domain: "e.test",
    });
    expect(result.query_id).toBe("q1");
    expect(result.entries[0]?.headword).toBe("hello");
    expect(result.sense_available).toBe(true);
  });

  it("anki 返回 note_id 与同语境标记", async () => {
    const { client } = clientWith(() => jsonResponse({ note_id: 42, exact_context_seen: true }));
    const result = await client.anki({
      word: "hello",
      definition_html: "<p>hi</p>",
      sentence: null,
      source: "Mini",
    });
    expect(result).toMatchObject({ note_id: 42, exact_context_seen: true });
  });

  it("sense 区分 202 pending 与 200 disabled/完成", async () => {
    const pending = clientWith(() => jsonResponse({ status: "pending" }, 202));
    await expect(pending.client.sense("q1")).resolves.toEqual({ status: "pending" });

    const disabled = clientWith(() => jsonResponse({ status: "disabled" }));
    await expect(disabled.client.sense("q1")).resolves.toEqual({ status: "disabled" });

    const resolved = clientWith(() =>
      jsonResponse({
        query_id: "q1",
        headword: "hello",
        status: "selected",
        selected_sense_id: "e0-s1",
        confidence: 0.8,
        selector_model: "jev-like",
        ai: null,
        error: null,
      }),
    );
    const result = await resolved.client.sense("q1");
    expect(result.status).toBe("resolved");
    if (result.status === "resolved") {
      expect(result.resolution.selected_sense_id).toBe("e0-s1");
    }
  });

  it("sense 未知查询 404 映射为 SERVER_ERROR", async () => {
    const { client } = clientWith(() =>
      jsonResponse({ code: "NOT_FOUND", message: "未知或已过期的查询" }, 404),
    );
    await expect(client.sense("nope")).rejects.toMatchObject({
      code: "SERVER_ERROR",
      status: 404,
    });
  });
});
