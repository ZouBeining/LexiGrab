import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RemoteSourceConfig } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  addSource: vi.fn(),
  updateSource: vi.fn(),
  testSourceQuery: vi.fn(),
  open: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  addSource: mocks.addSource,
  updateSource: mocks.updateSource,
  testSourceQuery: mocks.testSourceQuery,
  toIpcError: (cause: unknown) => cause,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: mocks.open }));

import { SourceDialog } from "./SourceDialog";

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

function renderAdd() {
  return render(
    <SourceDialog
      mode="add"
      source={null}
      existingSources={[]}
      onClose={vi.fn()}
      onSaved={vi.fn()}
    />,
  );
}

function selectRemote() {
  fireEvent.click(screen.getByRole("radio", { name: /远程 API/ }));
}

function fillName(value = "远程源") {
  fireEvent.change(screen.getByRole("textbox", { name: /名称/ }), { target: { value } });
}

afterEach(cleanup);

beforeEach(() => {
  mocks.addSource.mockReset().mockResolvedValue(REMOTE);
  mocks.updateSource.mockReset().mockResolvedValue(REMOTE);
  mocks.testSourceQuery.mockReset();
  mocks.open.mockReset();
});

describe("SourceDialog 远程源表单", () => {
  it("URL 模板缺少 {word} 时不保存并显示行内错误", async () => {
    renderAdd();
    selectRemote();
    fillName();
    fireEvent.change(screen.getByRole("textbox", { name: /URL 模板/ }), {
      target: { value: "https://api.example.com/lookup?q=" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(screen.getByText(/URL 模板必须恰好包含一个 \{word\}/)).toBeTruthy(),
    );
    expect(mocks.addSource).not.toHaveBeenCalled();
  });

  it("超时越界时不保存并显示错误", async () => {
    renderAdd();
    selectRemote();
    fillName();
    fireEvent.change(screen.getByRole("textbox", { name: /URL 模板/ }), {
      target: { value: "https://api.example.com/lookup?q={word}" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: /请求超时/ }), {
      target: { value: "100" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(screen.getByText(/超时需为 200–15000 的整数/)).toBeTruthy(),
    );
    expect(mocks.addSource).not.toHaveBeenCalled();
  });

  it("JSONPath 与脚本模式切换时替换输入并显示可信脚本警告", () => {
    renderAdd();
    selectRemote();
    expect(screen.getByRole("textbox", { name: /JSONPath 表达式/ })).toBeTruthy();
    expect(screen.queryByText("只运行可信脚本")).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "脚本" }));

    expect(screen.queryByRole("textbox", { name: /JSONPath 表达式/ })).toBeNull();
    expect(screen.getByRole("textbox", { name: /脚本路径/ })).toBeTruthy();
    expect(screen.getByText("只运行可信脚本")).toBeTruthy();
  });

  it("脚本模式缺少路径时阻止保存", async () => {
    renderAdd();
    selectRemote();
    fillName();
    fireEvent.change(screen.getByRole("textbox", { name: /URL 模板/ }), {
      target: { value: "https://api.example.com/lookup?q={word}" },
    });
    fireEvent.click(screen.getByRole("radio", { name: "脚本" }));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(screen.getByText("请输入脚本路径。")).toBeTruthy());
    expect(mocks.addSource).not.toHaveBeenCalled();
  });

  it("编辑远程源时保留 id 并按类型字段提交配置", async () => {
    render(
      <SourceDialog
        mode="edit"
        source={REMOTE}
        existingSources={[REMOTE]}
        onClose={vi.fn()}
        onSaved={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(mocks.updateSource).toHaveBeenCalledTimes(1));
    expect(mocks.updateSource).toHaveBeenCalledWith(
      REMOTE.id,
      expect.objectContaining({
        id: REMOTE.id,
        name: REMOTE.name,
        type: "remote_http",
        url_template: REMOTE.url_template,
        timeout_ms: 3000,
        response_parser: { mode: "jsonpath", expression: "$.entries[*]" },
        auth_header: null,
        auth_value: null,
      }),
    );
  });
});
