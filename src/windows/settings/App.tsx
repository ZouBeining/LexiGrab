import { useState } from "react";
import {
  applyTheme,
  DEFAULT_THEME,
  isTheme,
  readStoredTheme,
  storeTheme,
  type Theme,
} from "../../lib/theme";

const SECTIONS = [
  { id: "general", label: "通用" },
  { id: "anki", label: "Anki" },
  { id: "extension", label: "浏览器扩展" },
  { id: "cache", label: "缓存" },
  { id: "about", label: "关于" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

const THEME_LABELS: Record<Theme, string> = {
  figma: "Figma（默认）",
  apple: "Apple",
  cursor: "Cursor",
};

function readSection(): SectionId {
  const raw = new URLSearchParams(window.location.search).get("section");
  return SECTIONS.find((section) => section.id === raw)?.id ?? "general";
}

function navigate(section: SectionId): void {
  const url = new URL(window.location.href);
  url.searchParams.set("section", section);
  window.history.replaceState(null, "", url);
}

function GeneralSection() {
  const [theme, setTheme] = useState<Theme>(() => {
    const value = readStoredTheme();
    return value;
  });

  const selectTheme = (next: Theme) => {
    if (!isTheme(next)) return;
    setTheme(next);
    applyTheme(next);
    storeTheme(next);
  };

  return (
    <section aria-labelledby="general-title" className="flex max-w-[560px] flex-col gap-8">
      <header>
        <h1 id="general-title" className="text-[26px] font-medium tracking-tight">
          通用
        </h1>
        <p className="mt-1 text-sm text-ink-muted">主题切换立即生效，M1 接入配置持久化。</p>
      </header>

      <div className="flex flex-col gap-4">
        <span className="text-sm font-medium">主题</span>
        <div role="radiogroup" aria-label="主题" className="flex flex-col gap-2">
          {(Object.keys(THEME_LABELS) as Theme[]).map((item) => {
            const checked = item === theme;
            return (
              <label
                key={item}
                className={`flex h-12 cursor-pointer items-center justify-between rounded-xl border px-4 text-sm transition-colors duration-150 ${
                  checked ? "border-accent" : "border-hairline hover:bg-canvas-soft"
                }`}
              >
                <span>{THEME_LABELS[item]}</span>
                <input
                  type="radio"
                  name="theme"
                  value={item}
                  checked={checked}
                  onChange={() => selectTheme(item)}
                  className="accent-accent"
                />
              </label>
            );
          })}
        </div>
        <p className="text-xs text-ink-muted">默认主题为 {DEFAULT_THEME}，0.1.0 只提供浅色主题。</p>
      </div>
    </section>
  );
}

export function App() {
  const [section, setSection] = useState<SectionId>(readSection);

  const select = (next: SectionId) => {
    navigate(next);
    setSection(next);
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-[200px] shrink-0 flex-col border-r border-hairline bg-canvas-soft px-4 py-6">
        <p className="px-2 text-[16px] font-medium tracking-tight">设置</p>
        <nav aria-label="设置分区" className="mt-6 flex flex-col gap-1">
          {SECTIONS.map((item) => (
            <button
              key={item.id}
              type="button"
              aria-current={item.id === section ? "page" : undefined}
              onClick={() => select(item.id)}
              className={`h-10 rounded-xl px-3 text-left text-sm transition-colors duration-150 ${
                item.id === section
                  ? "bg-canvas font-medium text-ink ring-1 ring-hairline"
                  : "text-ink-muted hover:bg-canvas hover:text-ink"
              }`}
            >
              {item.label}
            </button>
          ))}
        </nav>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto px-10 py-8">
        {section === "general" ? (
          <GeneralSection />
        ) : (
          <section className="max-w-[560px]">
            <h1 className="text-[26px] font-medium tracking-tight">
              {SECTIONS.find((item) => item.id === section)?.label}
            </h1>
            <div className="mt-8 rounded-3xl border border-dashed border-hairline px-6 py-12 text-center text-sm text-ink-muted">
              该分区在后续里程碑接入。
            </div>
          </section>
        )}
      </main>
    </div>
  );
}
