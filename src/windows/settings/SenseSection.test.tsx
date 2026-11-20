// SPDX-License-Identifier: AGPL-3.0-only
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AiDefinition, SenseResolution, Settings } from "../../lib/types";

const mocks = vi.hoisted(() => ({
  updateSettings: vi.fn(),
  testSenseSelector: vi.fn(),
  testLlm: vi.fn(),
  clearSenseCache: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  updateSettings: mocks.updateSettings,
  testSenseSelector: mocks.testSenseSelector,
  testLlm: mocks.testLlm,
  clearSenseCache: mocks.clearSenseCache,
  toIpcError: (cause: unknown) => cause,
}));

import { SenseSection } from "./SenseSection";

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

const SELECTION: SenseResolution = {
  query_id: "q-1",
  headword: "bank",
  status: "selected",
  selected_sense_id: "e0-s2",
  confidence: 0.81,
  selector_model: "jev-like-test",
  ai: null,
  error: null,
  candidates: [],
};

const AI: AiDefinition = {
  definition_text: "河岸",
  part_of_speech: "noun",
  model: "test-model",
  confidence: 0.7,
  based_on_sense_id: null,
  generated_at: 1_700_000_000_000,
};

afterEach(cleanup);

beforeEach(() => {
  mocks.updateSettings.mockReset().mockImplementation(async (next: Settings) => next);
  mocks.testSenseSelector.mockReset().mockResolvedValue(SELECTION);
  mocks.testLlm.mockReset().mockResolvedValue(AI);
  mocks.clearSenseCache.mockReset().mockResolvedValue({ removed: 3 });
});

describe("SenseSection", () => {
  it("阈值不满足 0 < 低 < 高 ≤ 1 时阻止保存", async () => {
    render(<SenseSection settings={SETTINGS} onSaved={vi.fn()} />);

    fireEvent.change(screen.getByRole("textbox", { name: /低置信度阈值/ }), {
      target: { value: "0.9" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(screen.getAllByText(/0 < 低阈值 < 高阈值 ≤ 1/).length).toBeGreaterThan(0),
    );
    expect(mocks.updateSettings).not.toHaveBeenCalled();
  });

  it("保存时提交 sense 与 llm 配置", async () => {
    const onSaved = vi.fn();
    render(<SenseSection settings={SETTINGS} onSaved={onSaved} />);

    fireEvent.click(screen.getByRole("checkbox", { name: /启用语境辅助选择器/ }));
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledTimes(1));
    expect(mocks.updateSettings.mock.calls[0]?.[0]).toMatchObject({
      sense: { enabled: true, low_confidence: 0.55, high_confidence: 0.72 },
      llm: { enabled: false, model: "", max_tokens: 320 },
    });
    expect(onSaved).toHaveBeenCalled();
    await waitFor(() => expect(screen.getByText("已保存")).toBeTruthy());
  });

  it("允许远程选择器时显示“原句会发送到该服务”的二次确认", () => {
    render(<SenseSection settings={SETTINGS} onSaved={vi.fn()} />);

    expect(screen.queryByText("原句会发送到该服务")).toBeNull();
    fireEvent.click(screen.getByRole("checkbox", { name: /允许远程选择器/ }));
    expect(screen.getByText("原句会发送到该服务")).toBeTruthy();
  });

  it("测试选择器与 LLM 分别展示结果", async () => {
    render(<SenseSection settings={SETTINGS} onSaved={vi.fn()} />);

    fireEvent.change(screen.getByRole("textbox", { name: "测试词" }), {
      target: { value: "bank" },
    });
    fireEvent.change(screen.getByRole("textbox", { name: "测试原句" }), {
      target: { value: "He sat on the bank of the river." },
    });
    fireEvent.click(screen.getByRole("button", { name: "测试选择器" }));

    await waitFor(() => expect(mocks.testSenseSelector).toHaveBeenCalledTimes(1));
    expect(mocks.testSenseSelector).toHaveBeenCalledWith(
      "bank",
      "He sat on the bank of the river.",
    );
    expect(await screen.findByText("e0-s2")).toBeTruthy();
    expect(screen.getByText("0.81（模型自评）")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "测试 LLM" }));
    await waitFor(() => expect(mocks.testLlm).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("河岸")).toBeTruthy();
    expect(screen.getByText("AI 生成 · test-model · 可能不准确")).toBeTruthy();
  });

  it("清理语境缓存需要二次确认", async () => {
    render(<SenseSection settings={SETTINGS} onSaved={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "清除语境缓存" }));
    expect(mocks.clearSenseCache).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "确认清除语境缓存" }));
    await waitFor(() => expect(mocks.clearSenseCache).toHaveBeenCalledTimes(1));
    expect(await screen.findByText("已清除 3 条语境缓存。")).toBeTruthy();
  });
});
