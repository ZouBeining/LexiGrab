/**
 * 桌面端本地桥接客户端。契约见 docs/BACKEND_STRUCTURE.md §5。
 *
 * 只在 background 里调用：content script 不直接 fetch，避免把令牌暴露给页面。
 * 所有网络失败都归一化为 `BridgeError`，错误码固定，便于气泡显示可操作文案。
 */

/** 默认端口，与桌面端 `[bridge].port` 一致。 */
export const DEFAULT_PORT = 12787;

/** 单次请求超时；桌面端 `lookup_collect_all` 截止 3.5s。 */
export const REQUEST_TIMEOUT_MS = 5000;

export type BridgeErrorCode =
  | "NOT_CONFIGURED"
  | "DESKTOP_OFFLINE"
  | "AUTH_FAILED"
  | "ORIGIN_REJECTED"
  | "TIMEOUT"
  | "SERVER_ERROR"
  | "INVALID_RESPONSE";

export class BridgeError extends Error {
  readonly code: BridgeErrorCode;
  readonly status: number | null;

  constructor(code: BridgeErrorCode, message: string, status: number | null = null) {
    super(message);
    this.name = "BridgeError";
    this.code = code;
    this.status = status;
  }
}

export interface SelectionPayload {
  word: string;
  sentence: string | null;
  url: string | null;
  title: string | null;
  domain: string | null;
}

/** 与桌面端 `Entry` 同形；`definition_html` 已在服务端清洗并剥离本地媒体。 */
export interface BridgeEntry {
  headword: string;
  definition_html: string;
  phonetic: string | null;
  source_name: string;
}

export interface SelectionResult {
  query_id: string;
  entries: BridgeEntry[];
  source_names: string[];
  partial: boolean;
  sense_available: boolean;
  elapsed_ms: number;
}

export interface AnkiPayload {
  word: string;
  definition_html: string;
  sentence: string | null;
  source: string;
}

export interface AnkiResult {
  note_id: number;
  exact_context_seen: boolean;
}

export interface HealthResult {
  ok: boolean;
  app: string;
  version: string;
  api_version: number;
}

export interface SenseCandidateView {
  id: string;
  entry_index: number;
  source_name: string;
  headword: string;
  part_of_speech: string | null;
  definition_text: string;
}

export interface SenseResolutionView {
  query_id: string;
  headword: string;
  status: string;
  selected_sense_id: string | null;
  confidence: number | null;
  selector_model: string | null;
  ai: {
    definition_text: string;
    part_of_speech: string | null;
    model: string;
    confidence: number | null;
  } | null;
  error: string | null;
  candidates?: SenseCandidateView[];
}

export type SenseResult =
  | { status: "pending" }
  | { status: "disabled" }
  | { status: "resolved"; resolution: SenseResolutionView };

export interface BridgeClient {
  health(): Promise<HealthResult>;
  selection(payload: SelectionPayload): Promise<SelectionResult>;
  anki(payload: AnkiPayload): Promise<AnkiResult>;
  sense(queryId: string): Promise<SenseResult>;
}

export interface BridgeClientOptions {
  port: number;
  token: string;
  /** 测试注入；默认用全局 fetch。 */
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
}

export function bridgeBaseUrl(port: number): string {
  return `http://127.0.0.1:${port}`;
}

/**
 * 创建桥接客户端。令牌为空时所有请求都返回 `NOT_CONFIGURED`，
 * 让气泡提示“先配对”而不是“桌面端未运行”。
 */
export function createBridgeClient(options: BridgeClientOptions): BridgeClient {
  const timeoutMs = options.timeoutMs ?? REQUEST_TIMEOUT_MS;
  const fetchImpl = options.fetchImpl ?? globalThis.fetch.bind(globalThis);

  async function request(
    method: "GET" | "POST",
    path: string,
    body?: unknown,
  ): Promise<{ status: number; json: () => Promise<unknown> }> {
    const token = options.token.trim();
    if (token.length === 0) {
      throw new BridgeError("NOT_CONFIGURED", "尚未配对：请在扩展设置中填写配对令牌");
    }

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), timeoutMs);
    let response: Response;
    try {
      response = await fetchImpl(`${bridgeBaseUrl(options.port)}${path}`, {
        method,
        headers: {
          "x-auth-token": token,
          ...(body === undefined ? {} : { "content-type": "application/json" }),
        },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal: controller.signal,
        credentials: "omit",
        cache: "no-store",
      });
    } catch (cause) {
      if (controller.signal.aborted) {
        throw new BridgeError("TIMEOUT", "桌面端响应超时，请重试");
      }
      throw new BridgeError(
        "DESKTOP_OFFLINE",
        "无法连接桌面端：请确认 LexiGrab 正在运行且桥接已启用",
      );
    } finally {
      clearTimeout(timer);
    }

    if (response.status === 401) {
      throw new BridgeError("AUTH_FAILED", "令牌错误，请重新配对", 401);
    }
    if (response.status === 403) {
      throw new BridgeError("ORIGIN_REJECTED", "桌面端拒绝了扩展来源", 403);
    }
    if (response.status === 202) {
      return { status: 202, json: async () => ({ status: "pending" }) };
    }
    if (!response.ok) {
      const detail = await readErrorMessage(response);
      throw new BridgeError("SERVER_ERROR", detail, response.status);
    }
    return {
      status: response.status,
      json: () => response.json() as Promise<unknown>,
    };
  }

  async function parse<T>(response: { json: () => Promise<unknown> }): Promise<T> {
    try {
      return (await response.json()) as T;
    } catch {
      throw new BridgeError("INVALID_RESPONSE", "桌面端返回了无法解析的响应");
    }
  }

  return {
    async health() {
      const result = await parse<HealthResult>(await request("GET", "/v1/health"));
      if (typeof result.ok !== "boolean" || typeof result.api_version !== "number") {
        throw new BridgeError("INVALID_RESPONSE", "健康检查响应格式不正确");
      }
      return result;
    },

    async selection(payload: SelectionPayload) {
      const result = await parse<SelectionResult>(await request("POST", "/v1/selection", payload));
      if (typeof result.query_id !== "string" || !Array.isArray(result.entries)) {
        throw new BridgeError("INVALID_RESPONSE", "查询响应格式不正确");
      }
      return result;
    },

    async anki(payload: AnkiPayload) {
      const result = await parse<AnkiResult>(await request("POST", "/v1/anki", payload));
      if (typeof result.note_id !== "number") {
        throw new BridgeError("INVALID_RESPONSE", "制卡响应格式不正确");
      }
      return result;
    },

    async sense(queryId: string) {
      const body = await parse<Record<string, unknown>>(
        await request("GET", `/v1/sense/${encodeURIComponent(queryId)}`),
      );
      if (body.status === "pending" || body.status === "disabled") {
        return { status: body.status };
      }
      if (typeof body.query_id === "string") {
        return { status: "resolved", resolution: body as unknown as SenseResolutionView };
      }
      throw new BridgeError("INVALID_RESPONSE", "语境响应格式不正确");
    },
  };
}

/** 读取错误正文里的 `message`；正文不可读时给状态码。 */
async function readErrorMessage(response: Response): Promise<string> {
  try {
    const body = (await response.json()) as { message?: unknown };
    if (typeof body.message === "string" && body.message.length > 0) {
      return body.message;
    }
  } catch {
    // 落到状态码文案。
  }
  return `桌面端返回错误（HTTP ${response.status}）`;
}
