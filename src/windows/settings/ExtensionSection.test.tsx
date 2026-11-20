// SPDX-License-Identifier: AGPL-3.0-only
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BridgeStatus, Settings } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  getBridgeStatus: vi.fn(),
  getSettings: vi.fn(),
  regenerateBridgeToken: vi.fn(),
  updateSettings: vi.fn(),
  onBridgeStatus: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  getBridgeStatus: mocks.getBridgeStatus,
  getSettings: mocks.getSettings,
  regenerateBridgeToken: mocks.regenerateBridgeToken,
  updateSettings: mocks.updateSettings,
  toIpcError: (cause: unknown) => cause,
}));

vi.mock("../../lib/events", () => ({ onBridgeStatus: mocks.onBridgeStatus }));

import { ExtensionSection } from "./ExtensionSection";

const TOKEN = "a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90";

const SETTINGS = {
  meta: { schema_version: 2, device_id: "d" },
  general: {
    hotkey: "CmdOrCtrl+Shift+D",
    theme: "figma",
    query_strategy: "local_first",
    max_entries_per_source: 3,
    max_entries_total: 20,
    start_minimized: false,
  },
  anki: {
    endpoint: "http://localhost:8765",
    deck: "Default",
    model: "Basic",
    tags: [],
    duplicate_policy: "warn",
    field_map: { word: "Word", definition: "Definition", sentence: "Sentence", source: "Source" },
  },
  bridge: { enabled: true, port: 12787, token: TOKEN },
  cache: { ttl_hours: 24, negative_ttl_minutes: 5 },
  sense: {
    enabled: false,
    selector_endpoint: "http://127.0.0.1:8787/v1/select",
    selector_timeout_ms: 800,
    selector_allow_remote: false,
    low_confidence: 0.55,
    high_confidence: 0.72,
    max_senses: 20,
  },
  llm: {
    enabled: false,
    endpoint: "http://127.0.0.1:11434/v1/chat/completions",
    model: "",
    api_key_env: "LEXIGRAB_LLM_API_KEY",
    timeout_ms: 15000,
    max_tokens: 320,
    allow_remote: false,
    cache_ttl_hours: 168,
  },
  sources: [],
} as unknown as Settings;

const STATUS: BridgeStatus = {
  enabled: true,
  running: true,
  port: 12787,
  url: "http://127.0.0.1:12787",
  api_version: 1,
  last_success_ms: null,
  error: null,
};

afterEach(cleanup);

beforeEach(() => {
  mocks.getBridgeStatus.mockReset().mockResolvedValue(STATUS);
  mocks.getSettings.mockReset().mockResolvedValue(SETTINGS);
  mocks.regenerateBridgeToken.mockReset().mockResolvedValue("b".repeat(64));
  mocks.updateSettings.mockReset().mockImplementation(async (next: Settings) => next);
  mocks.onBridgeStatus.mockReset().mockResolvedValue(() => undefined);
});

describe("ExtensionSection", () => {
  it("显示运行状态、地址与遮罩令牌", async () => {
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    expect(screen.getByText(/http:\/\/127\.0\.0\.1:12787/)).toBeTruthy();
    const tokenInput = screen.getByLabelText("配对令牌") as HTMLInputElement;
    expect(tokenInput.type).toBe("password");
    expect(tokenInput.value).toBe(TOKEN);
  });

  it("显示按钮可切换令牌可见性", async () => {
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "显示" }));
    expect((screen.getByLabelText("配对令牌") as HTMLInputElement).type).toBe("text");
    fireEvent.click(screen.getByRole("button", { name: "隐藏" }));
    expect((screen.getByLabelText("配对令牌") as HTMLInputElement).type).toBe("password");
  });

  it("复制令牌写入剪贴板", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "复制" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(TOKEN));
    await waitFor(() => expect(screen.getByRole("button", { name: "已复制" })).toBeTruthy());
  });

  it("轮换令牌需要二次确认并刷新设置", async () => {
    const onSaved = vi.fn();
    render(<ExtensionSection settings={SETTINGS} onSaved={onSaved} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "重新生成令牌" }));
    expect(mocks.regenerateBridgeToken).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认轮换" }));
    await waitFor(() => expect(mocks.regenerateBridgeToken).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalledTimes(1));
    expect(onSaved).toHaveBeenCalled();
  });

  it("端口越界时阻止保存并提示", async () => {
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    const portInput = screen.getByRole("textbox", { name: "端口" });
    fireEvent.change(portInput, { target: { value: "80" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(screen.getByText(/端口需为 1024–65535 的整数/)).toBeTruthy());
    expect(mocks.updateSettings).not.toHaveBeenCalled();
  });

  it("保存时提交端口与启用状态", async () => {
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledTimes(1));
    expect(mocks.updateSettings.mock.calls[0]?.[0]).toMatchObject({
      bridge: { enabled: false, port: 12787 },
    });
    await waitFor(() => expect(screen.getByText("已保存")).toBeTruthy());
  });

  it("端口占用时在 role=alert 中给出动作与错误码", async () => {
    mocks.updateSettings.mockRejectedValue({
      code: "BRIDGE_PORT_IN_USE",
      message: "端口 12787 已被占用",
      detail: null,
    });
    render(<ExtensionSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("运行中")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("保存失败");
    expect(alert.textContent).toContain("端口 12787 已被占用");
    expect(alert.textContent).toContain("换一个端口后重试");
    expect(alert.textContent).toContain("BRIDGE_PORT_IN_USE");
  });
});
