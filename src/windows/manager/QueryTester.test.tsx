import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteSourceConfig, TestResult } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  listSources: vi.fn(),
  testSourceQuery: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  listSources: mocks.listSources,
  testSourceQuery: mocks.testSourceQuery,
  toIpcError: (cause: unknown) => cause,
}));

import { QueryTester } from "./QueryTester";

const REMOTE: RemoteSourceConfig = {
  id: "remote-00000001",
  name: "例词 API",
  type: "remote_http",
  url_template: "https://api.example.com/lookup?q={word}",
  response_parser: { mode: "jsonpath", expression: "$.entries[*]" },
  auth_header: null,
  auth_value: null,
  timeout_ms: 3000,
  enabled: true,
  priority: 10,
};

const FAILED: TestResult = {
  entries: [],
  source_name: "例词 API",
  elapsed_ms: 123,
  raw_preview: '{"error":"unauthorized"}',
  error: { code: "REMOTE_HTTP_ERROR", message: "远程返回 401，请检查鉴权头" },
};

const SUCCESS: TestResult = {
  entries: [
    {
      headword: "serendipity",
      definition_html: "<p>意外发现珍奇事物的本领</p>",
      phonetic: null,
      resources: [],
      source_name: "例词 API",
    },
  ],
  source_name: "例词 API",
  elapsed_ms: 87,
  raw_preview: '{"entries":[]}',
  error: null,
};

async function runQuery() {
  render(<QueryTester />);
  await waitFor(() =>
    expect(screen.getByRole("option", { name: /例词 API（远程）/ })).toBeTruthy(),
  );
  fireEvent.change(screen.getByLabelText("要查询的词"), {
    target: { value: "serendipity" },
  });
  fireEvent.click(screen.getByRole("button", { name: "查询" }));
}

afterEach(cleanup);

beforeEach(() => {
  mocks.listSources.mockReset().mockResolvedValue([REMOTE]);
  mocks.testSourceQuery.mockReset();
});

describe("QueryTester 远程结果", () => {
  it("显示远程错误、耗时与原始响应预览", async () => {
    mocks.testSourceQuery.mockResolvedValue(FAILED);
    await runQuery();

    await waitFor(() =>
      expect(screen.getAllByText(/远程返回 401/).length).toBeGreaterThan(0),
    );
    expect(screen.getByText("REMOTE_HTTP_ERROR")).toBeTruthy();
    expect(screen.getByText("123 ms")).toBeTruthy();
    expect(screen.getByText("查看原始响应（前 4KB）")).toBeTruthy();
    expect(screen.getByText(/unauthorized/)).toBeTruthy();
    expect(mocks.testSourceQuery).toHaveBeenCalledWith(REMOTE, "serendipity");
  });

  it("显示远程条目与耗时", async () => {
    mocks.testSourceQuery.mockResolvedValue(SUCCESS);
    await runQuery();

    await waitFor(() => expect(screen.getByText("serendipity")).toBeTruthy());
    expect(screen.getByText("意外发现珍奇事物的本领")).toBeTruthy();
    expect(screen.getAllByText("87 ms").length).toBeGreaterThan(0);
  });

  it("IPC 错误时显示错误码与消息", async () => {
    mocks.testSourceQuery.mockRejectedValue({
      code: "CONFIG_INVALID",
      message: "配置无效",
      detail: null,
    });
    await runQuery();

    await waitFor(() => expect(screen.getByText("配置无效")).toBeTruthy());
    expect(screen.getByText("CONFIG_INVALID")).toBeTruthy();
  });
});
