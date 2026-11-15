/** 页面元数据与浮层定位。 */

export interface PageMeta {
  url: string;
  title: string;
  domain: string;
}

/** 选区在视口中的位置，用于定位浮动按钮与气泡。 */
export interface AnchorRect {
  left: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export function pageMeta(doc: Document): PageMeta {
  const url = doc.location?.href ?? "";
  const title = doc.title ?? "";
  let domain = "";
  try {
    domain = new URL(url).hostname;
  } catch {
    domain = "";
  }
  return { url, title, domain };
}

export function rangeAnchor(range: Range): AnchorRect {
  const rect = range.getBoundingClientRect();
  if (rect.width === 0 && rect.height === 0) {
    // 有些页面 Range 返回空矩形，退到起点元素上。
    const element =
      range.startContainer.nodeType === Node.ELEMENT_NODE
        ? (range.startContainer as Element)
        : range.startContainer.parentElement;
    const fallback = element?.getBoundingClientRect();
    if (fallback) {
      return {
        left: fallback.left,
        top: fallback.top,
        bottom: fallback.bottom,
        width: fallback.width,
        height: fallback.height,
      };
    }
  }
  return {
    left: rect.left,
    top: rect.top,
    bottom: rect.bottom,
    width: rect.width,
    height: rect.height,
  };
}

export const BUTTON_SIZE = 28;
export const BUBBLE_WIDTH = 360;
export const BUBBLE_MAX_HEIGHT = 480;
const GAP = 8;
const EDGE = 8;

/** 浮动按钮位置：选区右下方，越界时回到视口内。 */
export function buttonPosition(anchor: AnchorRect, viewport: Viewport): { left: number; top: number } {
  const left = Math.min(
    Math.max(EDGE, anchor.left + anchor.width + GAP),
    Math.max(EDGE, viewport.width - BUTTON_SIZE - EDGE),
  );
  const top = Math.min(
    Math.max(EDGE, anchor.bottom + GAP),
    Math.max(EDGE, viewport.height - BUTTON_SIZE - EDGE),
  );
  return { left, top };
}

/** 气泡位置：下方空间不足时翻到上方，左右夹到视口内。 */
export function bubblePosition(
  anchor: AnchorRect,
  viewport: Viewport,
): { left: number; top: number; placement: "below" | "above" } {
  const maxLeft = Math.max(EDGE, viewport.width - BUBBLE_WIDTH - EDGE);
  const left = Math.min(Math.max(EDGE, anchor.left), maxLeft);
  const below = anchor.bottom + GAP;
  const fitsBelow = below + BUBBLE_MAX_HEIGHT <= viewport.height - EDGE;
  if (fitsBelow) {
    return { left, top: below, placement: "below" };
  }
  const above = anchor.top - GAP - BUBBLE_MAX_HEIGHT;
  if (above >= EDGE) {
    return { left, top: above, placement: "above" };
  }
  // 两侧都放不下：贴顶显示，交给 max-height 滚动。
  return { left, top: EDGE, placement: "below" };
}
