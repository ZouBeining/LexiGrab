import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Entry, SelectionContext, Settings } from "../../lib/types";

const ipc = vi.hoisted(() => ({
  checkCardContext: vi.fn(),
  exportToAnki: vi.fn(),
  getCurrentSelection: vi.fn(),
  getSettings: vi.fn(),
  hidePopup: vi.fn(),
  lookup: vi.fn(),
  openWindow: vi.fn(),
  setPopupInteractive: vi.fn(),
  toIpcError: vi.fn(() => ({ code: "INTERNAL", message: "发生未知错误", detail: null })),
}));

const events = vi.hoisted(() => ({
  onPopupSelection: vi.fn(() => Promise.resolve(() => undefined)),
  onPopupCaptureError: vi.fn(() => Promise.resolve(() => undefined)),
  onHotkeyError: vi.fn(() => Promise.resolve(() => undefined)),
}));

vi.mock("../../lib/ipc", () => ipc);
vi.mock("../../lib/events", () => events);

import { PopupApp } from "./PopupApp";

const selection: SelectionContext = {
  text: "serendipity",
  sentence: "It was pure serendipity that we met.",
  url: null,
  title: null,
  domain: null,
  anchor: null,
  foreground_hwnd: 1,
  app_name: "notepad.exe",
  method: "uia",
  captured_at_ms: 1,
  elapsed_ms: 10,
};

const entry: Entry = {
  headword: "serendipity",
  definition_html: "<div>意外发现珍奇事物的本领</div>",
  phonetic: "/ˌserənˈdɪpəti/",
  resources: [],
  source_name: "Mini",
};

function makeSettings(duplicatePolicy: "warn" | "allow"): Settings {
  return {
    meta: { schema_version: 2, device_id: "device" },
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
      duplicate_policy: duplicatePolicy,
      field_map: { word: "Word", definition: "Definition", sentence: "Sentence", source: "Source" },
    },
    bridge: { enabled: true, port: 12787, token: "" },
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
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  ipc.getCurrentSelection.mockResolvedValue(selection);
  ipc.getSettings.mockResolvedValue(makeSettings("warn"));
  ipc.lookup.mockResolvedValue([entry]);
  ipc.hidePopup.mockResolvedValue(undefined);
  ipc.openWindow.mockResolvedValue(undefined);
  ipc.setPopupInteractive.mockResolvedValue(undefined);
  ipc.exportToAnki.mockResolvedValue({ note_id: 123, exact_context_seen: false });
  ipc.checkCardContext.mockResolvedValue({ seen: false, note_id: null, created_at: null });
});

afterEach(cleanup);

describe("PopupApp", () => {
  it("renders results after the mount-time selection lookup", async () => {
    render(<PopupApp />);

    expect(screen.getByText("正在读取选区…")).toBeTruthy();
    expect(await screen.findByRole("heading", { name: "serendipity" })).toBeTruthy();
    expect(screen.getByText("意外发现珍奇事物的本领")).toBeTruthy();
    expect(screen.getByText("It was pure serendipity that we met.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "保存所选义项" })).toBeTruthy();
    expect(ipc.lookup).toHaveBeenCalledWith({ query: "serendipity", queryId: expect.any(String) });
  });

  it("shows the empty state with manual-card and dictionary actions", async () => {
    ipc.lookup.mockResolvedValue([]);
    render(<PopupApp />);

    expect(await screen.findByText("未找到释义")).toBeTruthy();
    expect(screen.getByRole("button", { name: "仍然制卡" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "添加词典" })).toBeTruthy();
  });

  it("shows the capture error state with retry when no selection exists", async () => {
    ipc.getCurrentSelection.mockResolvedValue(null);
    render(<PopupApp />);

    expect(await screen.findByText("未捕获到文本")).toBeTruthy();
    expect(screen.getByRole("button", { name: "重新捕获" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "打开词典管理" })).toBeTruthy();
  });

  it("shows the query error state and retries the lookup", async () => {
    ipc.lookup.mockRejectedValueOnce({ code: "DICT_BUSY", message: "词典正忙，请稍后重试。", detail: null });
    render(<PopupApp />);

    expect(await screen.findByText("查询失败")).toBeTruthy();
    ipc.lookup.mockResolvedValueOnce([entry]);
    fireEvent.click(screen.getByRole("button", { name: "重试" }));

    expect(await screen.findByRole("heading", { name: "serendipity" })).toBeTruthy();
    expect(ipc.lookup).toHaveBeenCalledTimes(2);
  });

  it("enters edit mode from empty and saves edited drafts", async () => {
    ipc.lookup.mockResolvedValue([]);
    render(<PopupApp />);

    fireEvent.click(await screen.findByRole("button", { name: "仍然制卡" }));
    expect(await screen.findByText("编辑模式")).toBeTruthy();
    expect(ipc.setPopupInteractive).toHaveBeenCalledWith(true);

    const sentence = screen.getByLabelText("原句");
    fireEvent.change(sentence, { target: { value: "手动补录的原句" } });
    fireEvent.click(screen.getByRole("button", { name: "保存卡片" }));

    await waitFor(() => expect(ipc.exportToAnki).toHaveBeenCalledTimes(1));
    expect(ipc.checkCardContext).toHaveBeenCalledWith(
      expect.objectContaining({ word: "serendipity", sentence: "手动补录的原句" }),
    );
  });

  it("warns on the same context and saves anyway", async () => {
    ipc.checkCardContext.mockResolvedValue({ seen: true, note_id: 42, created_at: 1 });
    render(<PopupApp />);

    fireEvent.click(await screen.findByRole("button", { name: "保存所选义项" }));
    expect(await screen.findByText(/此语境已存过/)).toBeTruthy();
    expect(ipc.exportToAnki).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "仍然保存" }));
    expect(await screen.findByText("已保存到 Anki")).toBeTruthy();
    expect(screen.getByText("note 123")).toBeTruthy();
    expect(ipc.exportToAnki).toHaveBeenCalledTimes(1);
    expect(ipc.exportToAnki).toHaveBeenCalledWith({
      word: "serendipity",
      definition_html: entry.definition_html,
      sentence: "It was pure serendipity that we met.",
      source: "Mini",
    });
    expect(ipc.checkCardContext).toHaveBeenCalledTimes(1);
  });

  it("skips the context check when duplicate_policy is allow", async () => {
    ipc.getSettings.mockResolvedValue(makeSettings("allow"));
    render(<PopupApp />);

    fireEvent.click(await screen.findByRole("button", { name: "保存所选义项" }));
    expect(await screen.findByText("已保存到 Anki")).toBeTruthy();
    expect(screen.getByRole("button", { name: "再存一张" })).toBeTruthy();
    expect(ipc.checkCardContext).not.toHaveBeenCalled();
    expect(ipc.exportToAnki).toHaveBeenCalledTimes(1);
  });

  it("returns to results after 再存一张", async () => {
    ipc.getSettings.mockResolvedValue(makeSettings("allow"));
    render(<PopupApp />);

    fireEvent.click(await screen.findByRole("button", { name: "保存所选义项" }));
    fireEvent.click(await screen.findByRole("button", { name: "再存一张" }));

    expect(await screen.findByRole("button", { name: "保存所选义项" })).toBeTruthy();
    expect(screen.queryByText("已保存到 Anki")).toBeNull();
  });
});
