// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import cssText from "./style.css?inline";
import { Bubble, type BubblePosition, type BubbleSnapshot } from "./Bubble";
import { buttonPosition, bubblePosition, pageMeta, rangeAnchor, type AnchorRect } from "../../lib/dom";
import { readSelectionSnapshot } from "../../lib/sentence";
import { sendRequest } from "../../lib/messages";

/** 宿主节点 id，用于避免重复挂载。 */
const HOST_ID = "lexigrab-overlay-host";
/** 选区稳定 120ms 后再生成快照（APP_FLOW §8.2）。 */
const CAPTURE_DELAY_MS = 120;

interface ActiveSelection {
  snapshot: BubbleSnapshot;
  anchor: AnchorRect;
}

export default defineContentScript({
  matches: ["<all_urls>"],
  runAt: "document_idle",
  main() {
    mount();
  },
});

/** 挂载 Shadow DOM 宿主；页面 CSS 与我们的样式互不影响。 */
function mount(): void {
  if (document.getElementById(HOST_ID)) return;
  const host = document.createElement("div");
  host.id = HOST_ID;
  const shadow = host.attachShadow({ mode: "open" });
  const style = document.createElement("style");
  style.textContent = cssText;
  const container = document.createElement("div");
  shadow.append(style, container);
  document.documentElement.append(host);
  createRoot(container).render(<Overlay host={host} />);
}

function viewport(): { width: number; height: number } {
  return { width: window.innerWidth, height: window.innerHeight };
}

function Overlay({ host }: { host: HTMLElement }) {
  const [active, setActive] = useState<ActiveSelection | null>(null);
  const [open, setOpen] = useState(false);
  const openRef = useRef(false);
  openRef.current = open;

  // 捕获：mouseup / keyup 后生成快照，气泡打开时不覆盖。
  useEffect(() => {
    let timer: number | undefined;
    const isOurs = (event: Event) => event.composedPath().includes(host);

    const handler = (event: Event) => {
      if (isOurs(event) || openRef.current) return;
      if (timer !== undefined) window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        const snapshot = readSelectionSnapshot(document);
        if (!snapshot) {
          setActive(null);
          return;
        }
        const selection = document.getSelection();
        if (!selection || selection.rangeCount === 0) {
          setActive(null);
          return;
        }
        setActive({
          snapshot: { ...snapshot, ...pageMeta(document) },
          anchor: rangeAnchor(selection.getRangeAt(0)),
        });
      }, CAPTURE_DELAY_MS);
    };

    document.addEventListener("mouseup", handler);
    document.addEventListener("keyup", handler);
    return () => {
      if (timer !== undefined) window.clearTimeout(timer);
      document.removeEventListener("mouseup", handler);
      document.removeEventListener("keyup", handler);
    };
  }, [host]);

  // 隐藏：Escape、点击外部、滚动。
  useEffect(() => {
    const isOurs = (event: Event) => event.composedPath().includes(host);
    const hide = () => {
      setOpen(false);
      setActive(null);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    const onPointerDown = (event: Event) => {
      if (!isOurs(event)) hide();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPointerDown, true);
    window.addEventListener("scroll", hide, true);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPointerDown, true);
      window.removeEventListener("scroll", hide, true);
    };
  }, [host]);

  if (!active) return null;

  if (!open) {
    const { left, top } = buttonPosition(active.anchor, viewport());
    return (
      <button
        type="button"
        aria-label="用 LexiGrab 查询所选单词"
        onClick={() => setOpen(true)}
        className="lg-interactive absolute flex size-7 items-center justify-center rounded-full bg-ink text-[10px] font-medium text-on-accent shadow-lg transition-transform hover:scale-105"
        style={{ left, top }}
      >
        LG
      </button>
    );
  }

  const position: BubblePosition = bubblePosition(active.anchor, viewport());
  return (
    <Bubble
      snapshot={active.snapshot}
      position={position}
      send={sendRequest}
      onClose={() => {
        setOpen(false);
        setActive(null);
      }}
    />
  );
}
