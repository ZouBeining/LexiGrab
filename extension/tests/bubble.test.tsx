// SPDX-License-Identifier: AGPL-3.0-only
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Bubble, type BubbleSnapshot } from "../entrypoints/content/Bubble";
import type { RuntimeRequest } from "../lib/messages";

const SNAPSHOT: BubbleSnapshot = {
  word: "serendipity",
  sentence: "It was pure serendipity that we met.",
  block_offset: 12,
  url: "https://example.com/article",
  title: "An article",
  domain: "example.com",
};

const POSITION = { left: 40, top: 80, placement: "below" as const };

function selectionResult(overrides: Record<string, unknown> = {}) {
  return {
    query_id: "q1",
    entries: [
      {
        headword: "serendipity",
        definition_html: "<p>serendipity definition</p>",
        phonetic: "/ˌserənˈdɪpəti/",
        source_name: "Mini",
      },
    ],
    source_names: ["Mini"],
    partial: false,
    sense_available: false,
    elapsed_ms: 12.5,
    ...overrides,
  };
}

function renderBubble(send: (request: RuntimeRequest) => Promise<unknown>) {
  return render(
    <Bubble snapshot={SNAPSHOT} position={POSITION} send={send} onClose={() => undefined} />,
  );
}

/** 等 React 处理完一轮微任务（不用 waitFor，避免与假定时器冲突）。 */
async function flush() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
}

describe("Bubble", () => {
  beforeEach(() => {
    document.body.innerHTML = "";
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("先显示加载态，再显示释义与来源", async () => {
    const send = vi.fn(async () => selectionResult());
    renderBubble(send);
    expect(screen.getByRole("status").textContent).toContain("正在查询");
    await flush();
    const dialog = screen.getByRole("dialog", { name: "LexiGrab 查询结果" });
    expect(dialog.textContent).toContain("serendipity definition");
    expect(dialog.textContent).toContain("Mini");
    expect(dialog.textContent).toContain("It was pure serendipity");
    expect(send).toHaveBeenCalledWith(
      expect.objectContaining({ type: "lexigrab:selection", payload: expect.objectContaining({ word: "serendipity" }) }),
    );
  });

  it("二次清洗释义 HTML，移除脚本", async () => {
    const send = vi.fn(async () =>
      selectionResult({
        entries: [
          {
            headword: "serendipity",
            definition_html: "<p>safe text</p><script>window.__pwned = 1</script>",
            phonetic: null,
            source_name: "Mini",
          },
        ],
      }),
    );
    renderBubble(send);
    await flush();
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("safe text");
    expect(dialog.querySelector("script")).toBeNull();
  });

  it("空结果显示提示", async () => {
    const send = vi.fn(async () => selectionResult({ entries: [], source_names: [] }));
    renderBubble(send);
    await flush();
    expect(screen.getByRole("dialog").textContent).toContain("未找到释义");
  });

  it("离线与令牌错误给出不同提示", async () => {
    const offline = vi.fn(async () => {
      throw { code: "DESKTOP_OFFLINE", message: "无法连接桌面端" };
    });
    renderBubble(offline);
    await flush();
    expect(screen.getByRole("dialog").textContent).toContain("桌面端未连接");

    document.body.innerHTML = "";
    const auth = vi.fn(async () => {
      throw { code: "AUTH_FAILED", message: "令牌错误" };
    });
    renderBubble(auth);
    await flush();
    expect(screen.getByRole("dialog").textContent).toContain("令牌错误");
  });

  it("保存后显示已保存，并允许再存一张（同词不阻断）", async () => {
    let ankiCalls = 0;
    const send = vi.fn(async (request: RuntimeRequest) => {
      if (request.type === "lexigrab:selection") return selectionResult();
      if (request.type === "lexigrab:anki") {
        ankiCalls += 1;
        return { note_id: 700 + ankiCalls, exact_context_seen: false };
      }
      throw new Error(`unexpected ${request.type}`);
    });
    renderBubble(send);
    await flush();

    act(() => {
      screen.getByRole("button", { name: "保存所选义项" }).click();
    });
    await flush();
    expect(screen.getByRole("dialog").textContent).toContain("已保存");
    expect(screen.getByRole("dialog").textContent).toContain("note_id 701");

    act(() => {
      screen.getByRole("button", { name: "再存一张" }).click();
    });
    act(() => {
      screen.getByRole("button", { name: "保存所选义项" }).click();
    });
    await flush();
    expect(ankiCalls).toBe(2);
    expect(screen.getByRole("dialog").textContent).toContain("note_id 702");
  });

  it("同语境已存时显示提示，但不阻断保存", async () => {
    const send = vi.fn(async (request: RuntimeRequest) => {
      if (request.type === "lexigrab:selection") return selectionResult();
      return { note_id: 9, exact_context_seen: true };
    });
    renderBubble(send);
    await flush();
    act(() => {
      screen.getByRole("button", { name: "保存所选义项" }).click();
    });
    await flush();
    const text = screen.getByRole("dialog").textContent ?? "";
    expect(text).toContain("已保存");
    expect(text).toContain("此前已保存过");
  });

  it("保存失败时按 ANKI_UNREACHABLE 给出可操作提示", async () => {
    const send = vi.fn(async (request: RuntimeRequest) => {
      if (request.type === "lexigrab:selection") return selectionResult();
      throw { code: "ANKI_UNREACHABLE", message: "Anki 未运行或 AnkiConnect 未安装" };
    });
    renderBubble(send);
    await flush();
    act(() => {
      screen.getByRole("button", { name: "保存所选义项" }).click();
    });
    await flush();
    const text = screen.getByRole("dialog").textContent ?? "";
    expect(text).toContain("Anki 未连接");
    expect(text).toContain("启动 Anki");
    expect(text).toContain("ANKI_UNREACHABLE");
  });

  it("语境轮询 pending 不干扰展示，完成后显示 AI 释义", async () => {
    vi.useFakeTimers();
    let senseAttempts = 0;
    const send = vi.fn(async (request: RuntimeRequest) => {
      if (request.type === "lexigrab:selection") return selectionResult({ sense_available: true });
      if (request.type === "lexigrab:sense") {
        senseAttempts += 1;
        if (senseAttempts < 2) return { status: "pending" };
        return {
          status: "resolved",
          resolution: {
            query_id: "q1",
            headword: "serendipity",
            status: "low_confidence",
            selected_sense_id: null,
            confidence: 0.4,
            selector_model: "jev-like",
            ai: {
              definition_text: "the faculty of making happy discoveries",
              part_of_speech: "noun",
              model: "llama-local",
              confidence: 0.7,
            },
            error: null,
          },
        };
      }
      throw new Error("unexpected");
    });

    renderBubble(send);
    await flush();
    expect(screen.getByRole("dialog").textContent).toContain("serendipity definition");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
      await Promise.resolve();
    });
    expect(screen.getByRole("dialog").textContent).toContain("正在获取语境建议");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
      await Promise.resolve();
    });
    const text = screen.getByRole("dialog").textContent ?? "";
    expect(text).toContain("AI 生成 · llama-local · 可能不准确");
    expect(text).toContain("the faculty of making happy discoveries");
    expect(senseAttempts).toBe(2);
  });

  it("语境 disabled 时不报错也不显示推荐区", async () => {
    vi.useFakeTimers();
    const send = vi.fn(async (request: RuntimeRequest) => {
      if (request.type === "lexigrab:selection") return selectionResult({ sense_available: false });
      if (request.type === "lexigrab:sense") return { status: "disabled" };
      throw new Error("unexpected");
    });
    renderBubble(send);
    await flush();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
      await Promise.resolve();
    });
    const text = screen.getByRole("dialog").textContent ?? "";
    expect(text).toContain("serendipity definition");
    expect(text).not.toContain("AI 释义");
  });
});
