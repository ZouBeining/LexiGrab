import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CacheStats, Settings } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  getCacheStats: vi.fn(),
  clearLookupCache: vi.fn(),
  clearSenseCache: vi.fn(),
  updateSettings: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  getCacheStats: mocks.getCacheStats,
  clearLookupCache: mocks.clearLookupCache,
  clearSenseCache: mocks.clearSenseCache,
  updateSettings: mocks.updateSettings,
  toIpcError: (cause: unknown) => cause,
}));

import { CacheSection } from "./CacheSection";

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
  bridge: { enabled: true, port: 12787, token: "a".repeat(64) },
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

const STATS: CacheStats = {
  entries: 5,
  positive_entries: 3,
  empty_entries: 2,
  sense_entries: 4,
  bytes: 2048,
};

afterEach(cleanup);

beforeEach(() => {
  mocks.getCacheStats.mockReset().mockResolvedValue(STATS);
  mocks.clearLookupCache.mockReset().mockResolvedValue({ removed: 5 });
  mocks.clearSenseCache.mockReset().mockResolvedValue({ removed: 4 });
  mocks.updateSettings.mockReset().mockImplementation(async (next: Settings) => next);
});

describe("CacheSection", () => {
  it("显示缓存统计与可读占用", async () => {
    render(<CacheSection settings={SETTINGS} onSaved={vi.fn()} />);

    await waitFor(() => expect(screen.getByText("5 条")).toBeTruthy());
    expect(screen.getByText("3 条")).toBeTruthy();
    expect(screen.getByText("2 条")).toBeTruthy();
    expect(screen.getByText("4 条")).toBeTruthy();
    expect(screen.getByText("2.0 KB")).toBeTruthy();
    expect(screen.getByText(/不会删除已保存卡片的历史记录（saved_cards）/)).toBeTruthy();
  });

  it("清除词典缓存需要二次确认并刷新统计", async () => {
    render(<CacheSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("5 条")).toBeTruthy());
    expect(mocks.getCacheStats).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "清除词典缓存" }));
    expect(mocks.clearLookupCache).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "确认清除词典缓存" }));
    await waitFor(() => expect(mocks.clearLookupCache).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(mocks.getCacheStats).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("已清除 5 条词典缓存。")).toBeTruthy();
    expect(mocks.clearSenseCache).not.toHaveBeenCalled();
  });

  it("清除语境缓存使用独立入口", async () => {
    render(<CacheSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("5 条")).toBeTruthy());

    fireEvent.click(screen.getByRole("button", { name: "清除语境缓存" }));
    expect(mocks.clearSenseCache).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认清除语境缓存" }));

    await waitFor(() => expect(mocks.clearSenseCache).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("已清除 4 条语境缓存。")).toBeTruthy();
    expect(mocks.clearLookupCache).not.toHaveBeenCalled();
  });

  it("TTL 越界时阻止保存", async () => {
    render(<CacheSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("5 条")).toBeTruthy());

    fireEvent.change(screen.getByRole("textbox", { name: /^缓存时长/ }), {
      target: { value: "0" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(screen.getByText(/缓存时长需为 1–720 小时的整数/)).toBeTruthy());
    expect(mocks.updateSettings).not.toHaveBeenCalled();
  });

  it("保存时提交缓存 TTL", async () => {
    render(<CacheSection settings={SETTINGS} onSaved={vi.fn()} />);
    await waitFor(() => expect(screen.getByText("5 条")).toBeTruthy());

    fireEvent.change(screen.getByRole("textbox", { name: /负缓存时长/ }), {
      target: { value: "10" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledTimes(1));
    expect(mocks.updateSettings.mock.calls[0]?.[0]).toMatchObject({
      cache: { ttl_hours: 24, negative_ttl_minutes: 10 },
    });
  });
});
