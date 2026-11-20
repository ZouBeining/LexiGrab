import { useCallback, useEffect, useState } from "react";
import { getSettings, toIpcError } from "../../lib/ipc";
import { applyTheme, isTheme } from "../../lib/theme";
import type { IpcError, Settings } from "../../lib/types";
import { Button } from "../../components/Button";
import { Spinner } from "../../components/Spinner";
import { AboutSection } from "./AboutSection";
import { AnkiSection } from "./AnkiSection";
import { CacheSection } from "./CacheSection";
import { ExtensionSection } from "./ExtensionSection";
import { GeneralSection } from "./GeneralSection";
import { SenseSection } from "./SenseSection";

const SECTIONS = [
  { id: "general", label: "通用" },
  { id: "anki", label: "Anki" },
  { id: "sense", label: "语境辅助" },
  { id: "extension", label: "浏览器扩展" },
  { id: "cache", label: "缓存" },
  { id: "about", label: "关于" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

const DEFAULT_SECTION: SectionId = "general";

function readSection(): SectionId {
  const raw = new URLSearchParams(window.location.search).get("section");
  return SECTIONS.find((section) => section.id === raw)?.id ?? DEFAULT_SECTION;
}

/** Updates only the `section` query parameter (no router library, APP_FLOW §1). */
function navigate(section: SectionId): void {
  const url = new URL(window.location.href);
  if (section === DEFAULT_SECTION) {
    url.searchParams.delete("section");
  } else {
    url.searchParams.set("section", section);
  }
  window.history.replaceState(null, "", url);
}

/**
 * Settings shell: 200px sidebar with six sections and a shared settings load.
 * Sections receive the loaded config and report saved updates back here.
 */
export function SettingsApp() {
  const [section, setSection] = useState<SectionId>(readSection);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<IpcError | null>(null);
  const [loading, setLoading] = useState(true);
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    let active = true;
    getSettings()
      .then((value) => {
        if (!active) return;
        setSettings(value);
        if (isTheme(value.general.theme)) applyTheme(value.general.theme);
      })
      .catch((cause: unknown) => {
        if (active) setError(toIpcError(cause));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [nonce]);

  const select = (next: SectionId) => {
    navigate(next);
    setSection(next);
  };

  const retry = useCallback(() => {
    setLoading(true);
    setError(null);
    setNonce((value) => value + 1);
  }, []);

  const renderSection = () => {
    if (section === "about") return <AboutSection />;

    if (loading) {
      return (
        <div className="flex items-center gap-2 border-y border-hairline py-8">
          <Spinner label="正在读取设置…" />
        </div>
      );
    }
    if (error) {
      return (
        <div role="alert" className="rounded-3xl border border-danger/40 px-6 py-6">
          <p className="text-sm font-medium text-danger">无法读取设置</p>
          <p className="mt-1 text-sm text-ink-muted">{error.message}</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">{error.code}</p>
          <Button variant="secondary" size="sm" className="mt-3" onClick={retry}>
            重试
          </Button>
        </div>
      );
    }
    if (!settings) return null;

    if (section === "general") {
      return <GeneralSection settings={settings} onSaved={setSettings} />;
    }
    if (section === "anki") {
      return <AnkiSection settings={settings} onSaved={setSettings} />;
    }
    if (section === "sense") {
      return <SenseSection settings={settings} onSaved={setSettings} />;
    }
    if (section === "extension") {
      return <ExtensionSection settings={settings} onSaved={setSettings} />;
    }
    return <CacheSection settings={settings} onSaved={setSettings} />;
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-[200px] shrink-0 flex-col border-r border-hairline bg-canvas-soft px-4 py-6">
        <p className="px-2 text-[16px] font-medium tracking-tight">设置</p>
        <nav aria-label="设置分区" className="mt-6 flex flex-col gap-1">
          {SECTIONS.map((item) => {
            const active = item.id === section;
            return (
              <button
                key={item.id}
                type="button"
                aria-current={active ? "page" : undefined}
                onClick={() => select(item.id)}
                className={`h-10 rounded-xl px-3 text-left text-sm transition-colors duration-150 ${
                  active
                    ? "bg-canvas font-medium text-ink ring-1 ring-hairline"
                    : "text-ink-muted hover:bg-canvas hover:text-ink"
                }`}
              >
                {item.label}
              </button>
            );
          })}
        </nav>
        <p className="mt-auto px-2 font-mono text-[11px] leading-5 text-ink-muted">LexiGrab</p>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto px-10 py-8">{renderSection()}</main>
    </div>
  );
}
