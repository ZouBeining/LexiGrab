export const THEMES = ["figma", "apple", "cursor"] as const;

export type Theme = (typeof THEMES)[number];

export const DEFAULT_THEME: Theme = "figma";

const STORAGE_KEY = "lexigrab.theme";

export function isTheme(value: unknown): value is Theme {
  return typeof value === "string" && (THEMES as readonly string[]).includes(value);
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

export function readStoredTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    return isTheme(stored) ? stored : DEFAULT_THEME;
  } catch {
    return DEFAULT_THEME;
  }
}

export function storeTheme(theme: Theme): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // 存储不可用时仅本次生效，不影响页面。
  }
}
