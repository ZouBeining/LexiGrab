// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it } from "vitest";
import { applyTheme, DEFAULT_THEME, isTheme, readStoredTheme, storeTheme } from "./theme";

describe("theme", () => {
  beforeEach(() => {
    window.localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("默认主题是 figma", () => {
    expect(DEFAULT_THEME).toBe("figma");
    expect(readStoredTheme()).toBe("figma");
  });

  it("只接受已知主题值", () => {
    expect(isTheme("apple")).toBe(true);
    expect(isTheme("dark")).toBe(false);
  });

  it("应用主题只改 data-theme", () => {
    applyTheme("cursor");
    expect(document.documentElement.dataset.theme).toBe("cursor");
  });

  it("读写本地存储", () => {
    storeTheme("apple");
    expect(readStoredTheme()).toBe("apple");
  });
});
