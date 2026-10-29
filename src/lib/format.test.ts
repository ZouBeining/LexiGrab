import { describe, expect, it } from "vitest";
import { formatElapsedMs, formatEntryCount, formatTimestamp, truncate } from "./format";

describe("formatElapsedMs", () => {
  it("小于 1 秒用毫秒", () => {
    expect(formatElapsedMs(0)).toBe("0 ms");
    expect(formatElapsedMs(42)).toBe("42 ms");
    expect(formatElapsedMs(42.4)).toBe("42 ms");
    expect(formatElapsedMs(999)).toBe("999 ms");
  });

  it("大于等于 1 秒用一位小数秒", () => {
    expect(formatElapsedMs(1000)).toBe("1.0 s");
    expect(formatElapsedMs(1234)).toBe("1.2 s");
    expect(formatElapsedMs(59_940)).toBe("59.9 s");
  });

  it("非法值返回占位符", () => {
    expect(formatElapsedMs(Number.NaN)).toBe("—");
    expect(formatElapsedMs(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatElapsedMs(-5)).toBe("—");
  });
});

describe("formatEntryCount", () => {
  it("输出中文条数", () => {
    expect(formatEntryCount(0)).toBe("0 条");
    expect(formatEntryCount(3)).toBe("3 条");
    expect(formatEntryCount(3.6)).toBe("4 条");
  });

  it("非法值返回占位符，负数归零", () => {
    expect(formatEntryCount(Number.NaN)).toBe("—");
    expect(formatEntryCount(-2)).toBe("0 条");
  });
});

describe("formatTimestamp", () => {
  it("格式化为本地时间 YYYY-MM-DD HH:mm", () => {
    const ms = new Date(2026, 0, 2, 3, 4, 5).getTime();
    expect(formatTimestamp(ms)).toBe("2026-01-02 03:04");
  });

  it("非法值返回占位符", () => {
    expect(formatTimestamp(Number.NaN)).toBe("—");
    expect(formatTimestamp(Number.POSITIVE_INFINITY)).toBe("—");
  });
});

describe("truncate", () => {
  it("未超出长度时原样返回", () => {
    expect(truncate("hello", 5)).toBe("hello");
    expect(truncate("hello", 10)).toBe("hello");
  });

  it("超出时截断并追加省略号", () => {
    expect(truncate("hello world", 8)).toBe("hello w…");
    expect(truncate("hello", 1)).toBe("…");
  });

  it("按码点计数，不切坏 emoji", () => {
    expect(truncate("😀😀😀", 2)).toBe("😀…");
    expect(Array.from(truncate("😀😀😀", 3))).toHaveLength(3);
  });

  it("非正上限返回空串", () => {
    expect(truncate("hello", 0)).toBe("");
    expect(truncate("hello", -3)).toBe("");
    expect(truncate("hello", Number.NaN)).toBe("");
  });
});
