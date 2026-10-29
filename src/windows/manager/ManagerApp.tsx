import { useEffect, useState } from "react";
import { getRuntimeStatus, getSettings } from "../../lib/ipc";
import { applyTheme, isTheme } from "../../lib/theme";
import type { RuntimeStatus } from "../../lib/types";
import { QueryTester } from "./QueryTester";
import { SourceList } from "./SourceList";
import { StatusView } from "./StatusView";

const VIEWS = [
  { id: "sources", label: "词典源" },
  { id: "test", label: "查询测试" },
  { id: "status", label: "运行状态" },
] as const;

type ViewId = (typeof VIEWS)[number]["id"];

const DEFAULT_VIEW: ViewId = "sources";

/** One-shot localStorage flag set by the settings window's "reopen guide" action. */
const FORCE_GUIDE_KEY = "lexigrab.guide.force";

/** Reads the active view from the URL query; no router library is used. */
function readView(): ViewId {
  const raw = new URLSearchParams(window.location.search).get("view");
  return VIEWS.find((view) => view.id === raw)?.id ?? DEFAULT_VIEW;
}

/**
 * Pure check for a first-run guide request: `?view=sources&guide=1` or the
 * one-shot localStorage handoff from the settings window.
 */
function isGuideRequested(): boolean {
  if (new URLSearchParams(window.location.search).get("guide") === "1") return true;
  try {
    return window.localStorage.getItem(FORCE_GUIDE_KEY) === "1";
  } catch {
    return false;
  }
}

/** Clears the one-shot storage handoff; the URL flag is not persisted. */
function consumeGuideHandoff(): boolean {
  try {
    if (window.localStorage.getItem(FORCE_GUIDE_KEY) !== "1") return false;
    window.localStorage.removeItem(FORCE_GUIDE_KEY);
    return true;
  } catch {
    return false;
  }
}

/** Updates only the `view` query parameter so refresh restores the same panel. */
function navigate(view: ViewId): void {
  const url = new URL(window.location.href);
  if (view === DEFAULT_VIEW) {
    url.searchParams.delete("view");
  } else {
    url.searchParams.set("view", view);
  }
  // Keep `guide` when it was requested explicitly; otherwise drop stale flags.
  if (url.searchParams.get("guide") !== "1") {
    url.searchParams.delete("guide");
  }
  window.history.replaceState(null, "", url);
}

function NavItem({ label, active, onSelect }: { label: string; active: boolean; onSelect: () => void }) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      onClick={onSelect}
      className={`flex h-11 items-center rounded-xl px-3 text-left text-sm transition-colors duration-150 ${
        active
          ? "bg-canvas font-medium text-ink ring-1 ring-hairline"
          : "text-ink-muted hover:bg-canvas hover:text-ink"
      }`}
    >
      {label}
    </button>
  );
}

/**
 * Manager shell: 220px sidebar plus a max-760px content column.
 * Views share the URL query navigation contract from docs/APP_FLOW.md §1.
 */
export function ManagerApp() {
  const [view, setView] = useState<ViewId>(readView);
  const [forceGuide, setForceGuide] = useState(isGuideRequested);
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);

  // Consume the one-shot storage handoff after mount (URL flags are not persisted).
  useEffect(() => {
    consumeGuideHandoff();
  }, []);

  useEffect(() => {
    let active = true;
    getRuntimeStatus()
      .then((value) => {
        if (active) setRuntime(value);
      })
      .catch(() => {
        // The footer version is decorative; a failure must not block the window.
      });
    return () => {
      active = false;
    };
  }, []);

  // Reflect the persisted theme saved from the settings window.
  useEffect(() => {
    let active = true;
    getSettings()
      .then((settings) => {
        if (active && isTheme(settings.general.theme)) applyTheme(settings.general.theme);
      })
      .catch(() => {
        // Theme sync is best-effort; the stored local theme stays in effect.
      });
    return () => {
      active = false;
    };
  }, []);

  // Settings hands off a one-shot "reopen guide" flag; consume it when the
  // manager window is brought to the front (no cross-window IPC needed).
  useEffect(() => {
    const onWindowFocus = () => {
      if (!consumeGuideHandoff()) return;
      setForceGuide(true);
      setView("sources");
      navigate("sources");
    };
    window.addEventListener("focus", onWindowFocus);
    return () => window.removeEventListener("focus", onWindowFocus);
  }, []);

  const select = (next: ViewId) => {
    navigate(next);
    setView(next);
    setForceGuide(false);
  };

  return (
    <div className="flex h-full">
      <aside className="flex w-[220px] shrink-0 flex-col border-r border-hairline bg-canvas-soft px-4 py-6">
        <div className="px-2">
          <p className="text-[16px] font-medium tracking-tight">LexiGrab</p>
          <p className="mt-1 font-mono text-[11px] tracking-[0.6px] text-ink-muted">词典管理器</p>
        </div>
        <nav aria-label="主导航" className="mt-8 flex flex-col gap-1">
          {VIEWS.map((item) => (
            <NavItem
              key={item.id}
              label={item.label}
              active={item.id === view}
              onSelect={() => select(item.id)}
            />
          ))}
        </nav>
        <p className="mt-auto px-2 font-mono text-[11px] leading-5 text-ink-muted">
          {runtime ? `v${runtime.version}` : "v0.1.0"} · Windows
        </p>
      </aside>

      <main className="min-w-0 flex-1 overflow-y-auto px-10 py-8">
        <div className="max-w-[760px]">
          {view === "sources" ? (
            <SourceList forceGuide={forceGuide} onGuideClosed={() => setForceGuide(false)} />
          ) : null}
          {view === "test" ? <QueryTester /> : null}
          {view === "status" ? <StatusView /> : null}
        </div>
      </main>
    </div>
  );
}
