// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  collectBlockText,
  findBlock,
  normalizeWord,
  pickSentence,
  segmentSentences,
  snapshotFromRange,
} from "../lib/sentence";

const FILLER =
  "The quick brown fox jumps over the lazy dog while the curious cat watches quietly from the fence. ";

function selectText(root: ParentNode, needle: string): Range {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const value = node.nodeValue ?? "";
    const index = value.indexOf(needle);
    if (index >= 0) {
      const range = document.createRange();
      range.setStart(node, index);
      range.setEnd(node, index + needle.length);
      return range;
    }
  }
  throw new Error(`selected text not found: ${needle}`);
}

function selectAcross(root: Node, startNeedle: string, endNeedle: string): Range {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let start: { node: Text; index: number } | null = null;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const value = node.nodeValue ?? "";
    const index = value.indexOf(startNeedle);
    if (index >= 0) {
      start = { node: node as Text, index };
      break;
    }
  }
  if (!start) throw new Error("start not found");
  const endIndex = (start.node.nodeValue ?? "").indexOf(endNeedle);
  if (endIndex < 0) throw new Error("end not found");
  const range = document.createRange();
  range.setStart(start.node, start.index);
  range.setEnd(start.node, endIndex + endNeedle.length);
  return range;
}

describe("normalizeWord", () => {
  it("接受单词与中文词组", () => {
    expect(normalizeWord("hello")).toBe("hello");
    expect(normalizeWord("  serendipity ")).toBe("serendipity");
    expect(normalizeWord("苹果")).toBe("苹果");
  });

  it("拒绝空文本、超长、换行与非字母内容", () => {
    expect(normalizeWord("   ")).toBeNull();
    expect(normalizeWord("a".repeat(65))).toBeNull();
    expect(normalizeWord("first\nsecond")).toBeNull();
    expect(normalizeWord("12345")).toBeNull();
  });
});

describe("findBlock and collectBlockText", () => {
  it("从文本节点向上找块级祖先", () => {
    document.body.innerHTML = "<div id='outer'><p id='target'>hello world</p></div>";
    const text = document.querySelector("#target")?.firstChild;
    expect(text).toBeTruthy();
    const block = findBlock(text as Node, document);
    expect(block?.tagName).toBe("P");
  });

  it("折叠空白并跳过隐藏元素", () => {
    document.body.innerHTML =
      "<p id='b'>Visible   text  one. <span style='display:none'>hidden words</span> More\n text.</p>";
    const block = document.querySelector("#b") as Element;
    const { text } = collectBlockText(block);
    expect(text).toBe("Visible text one. More text.");
    expect(text).not.toContain("hidden");
  });
});

describe("snapshotFromRange", () => {
  it("普通段落取包含选词的句子", () => {
    document.body.innerHTML = `<p id="b">${FILLER}${FILLER}Which serendipity brought us here exactly today. ${FILLER}</p>`;
    const range = selectText(document.body, "serendipity");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot).not.toBeNull();
    expect(snapshot?.word).toBe("serendipity");
    expect(snapshot?.sentence).toContain("serendipity");
    expect(snapshot?.sentence).toContain("Which");
    expect(snapshot?.sentence).not.toContain("quick brown fox");
    expect(snapshot!.sentence.length).toBeLessThanOrEqual(600);
  });

  it("嵌套 span 与链接内选词", () => {
    document.body.innerHTML = `<p id="b">${FILLER}${FILLER}We met <a href="#x"><span>serendipity</span></a> again on Tuesday. ${FILLER}</p>`;
    const range = selectText(document.body, "serendipity");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot?.word).toBe("serendipity");
    expect(snapshot?.sentence).toContain("Tuesday");
  });

  it("中文标点与长段落", () => {
    const chinese =
      "这是一个很长的中文段落，用来测试句子切分。".repeat(8) +
      "我们在这里遇到幸运的巧合，然后继续阅读。";
    document.body.innerHTML = `<p id="b">${chinese}${"后面的内容继续补充说明情况。".repeat(6)}</p>`;
    const range = selectText(document.body, "幸运的巧合");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot).not.toBeNull();
    expect(snapshot?.word).toBe("幸运的巧合");
    expect(snapshot?.sentence).toContain("幸运的巧合");
    expect(snapshot!.sentence.length).toBeLessThanOrEqual(600);
  });

  it("多词选区也能取到含选词的句子", () => {
    document.body.innerHTML = `<p id="b">${FILLER}${FILLER}The quick brown fox jumps over the lazy dog again and again. ${FILLER}</p>`;
    const range = selectAcross(document.body, "quick brown", "brown fox");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot?.word).toBe("quick brown fox");
    expect(snapshot?.sentence).toContain("quick brown fox");
  });

  it("没有标点的长文本退化为包含选词的片段", () => {
    const words = "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor ";
    document.body.innerHTML = `<p id="b">${words}${words}${words}"serendipity"${words}${words}</p>`;
    const range = selectText(document.body, "serendipity");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot?.sentence).toContain("serendipity");
    expect(snapshot!.sentence.length).toBeLessThanOrEqual(600);
  });

  it("短块直接用整块文本", () => {
    document.body.innerHTML = "<p>Short block with serendipity inside.</p>";
    const range = selectText(document.body, "serendipity");
    const snapshot = snapshotFromRange(range, document);
    expect(snapshot?.sentence).toBe("Short block with serendipity inside.");
  });

  it("密码输入框与折叠选区被忽略", () => {
    document.body.innerHTML = "<div><input type='password' /><p>serendipity text</p></div>";
    const input = document.querySelector("input") as HTMLInputElement;
    const range = document.createRange();
    range.setStart(input, 0);
    range.setEnd(input, 0);
    expect(snapshotFromRange(range, document)).toBeNull();
  });

  it("固定样本集中至少 90% 能取到含选词的原句", () => {
    const samples = [
      `<p>${FILLER}${FILLER}Alpha beta gamma serendipity delta epsilon zeta eta theta iota. ${FILLER}</p>`,
      `<p>${FILLER}${FILLER}Second sample mentions serendipity inside a longer paragraph here. ${FILLER}</p>`,
      `<div>${FILLER}${FILLER}Third sample: serendipity appears after a colon and before a comma, okay. ${FILLER}</div>`,
      `<li>${FILLER}${FILLER}Fourth sample has serendipity in a list item with enough text. ${FILLER}</li>`,
      `<blockquote>${FILLER}${FILLER}Fifth sample quotes serendipity in a block quote for length. ${FILLER}</blockquote>`,
      `<article>${FILLER}${FILLER}Sixth sample wraps serendipity in an article element. ${FILLER}</article>`,
      `<section>${FILLER}${FILLER}Seventh sample keeps serendipity inside a section. ${FILLER}</section>`,
      `<h2>${FILLER}${FILLER}Eighth sample heading with serendipity and more words. ${FILLER}</h2>`,
      `<table><tbody><tr><td>${FILLER}${FILLER}Ninth sample cell with serendipity in a table cell. ${FILLER}</td></tr></tbody></table>`,
      `<dl><dd>${FILLER}${FILLER}Tenth sample definition with serendipity and trailing text. ${FILLER}</dd></dl>`,
    ];
    let hits = 0;
    for (const html of samples) {
      document.body.innerHTML = html;
      const range = selectText(document.body, "serendipity");
      const snapshot = snapshotFromRange(range, document);
      if (snapshot?.sentence.includes("serendipity")) hits += 1;
    }
    expect(hits / samples.length).toBeGreaterThanOrEqual(0.9);
  });
});

describe("segmentSentences fallback", () => {
  it("缺少 Intl.Segmenter 时按标点切分", () => {
    const intl = Intl as unknown as Record<string, unknown>;
    const original = intl.Segmenter;
    try {
      delete intl.Segmenter;
      const segments = segmentSentences("First one. Second one! 第三句。");
      expect(segments).toHaveLength(3);
      const text = "First one. Second one! 第三句。";
      expect(text.slice(segments[1]?.start, segments[1]?.end)).toBe(" Second one!");
    } finally {
      intl.Segmenter = original;
    }
  });

  it("句段切分后能选中第二句", () => {
    const first = "First sentence without the target word at all but long enough. ";
    const second = "Second sentence mentions serendipity right here in the middle. ";
    const text = `${first.repeat(3)}${second}${first.repeat(3)}`;
    const index = text.indexOf("serendipity");
    const sentence = pickSentence(text, index, "serendipity");
    expect(sentence).toContain("serendipity");
    expect(sentence).not.toContain("First sentence");
  });
});
