/**
 * 选区与原句提取（PLAN_M3 T3.3）。纯 DOM 逻辑，方便 happy-dom 单元测试。
 *
 * 算法参考 references/ODH 的 `text.js`/`range.js` 思路：从选区起点向上找块级
 * 祖先，用 Range 前缀算原始偏移，再按句子边界取句；不复制其代码或类结构。
 * 优先 `Intl.Segmenter`，不可用时按中英文标点回退。
 */

/** 块级祖先选择器；到 body 停止。 */
export const BLOCK_SELECTOR =
  "p,li,td,th,blockquote,dd,dt,figcaption,article,section,main,div,h1,h2,h3,h4,h5,h6,pre";

/** 选中词最大长度（字符）。 */
export const MAX_WORD_CHARS = 64;
/** 原句最大长度（字符）。 */
export const MAX_SENTENCE_CHARS = 600;
/** 块文本不超过该长度时直接用整块。 */
export const WHOLE_BLOCK_CHARS = 300;

/** 提取结果：词、原句与选词在块内的折叠偏移。 */
export interface SentenceSnapshot {
  word: string;
  sentence: string;
  block_offset: number;
}

/** 判断是否值得处理的选中文本：1–64 字符、无换行、含字母或 CJK。 */
export function normalizeWord(raw: string): string | null {
  if (/[\r\n]/.test(raw)) {
    return null;
  }
  const word = raw.replace(/\s+/g, " ").trim();
  if (word.length === 0 || word.length > MAX_WORD_CHARS) {
    return null;
  }
  if (!/\p{L}/u.test(word)) {
    return null;
  }
  return word;
}

/** 密码输入框里的选区不处理。 */
export function isPasswordSelection(range: Range): boolean {
  const node = range.startContainer;
  const element = node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  if (!element) return false;
  if (element.closest("input[type='password'], textarea[type='password']")) return true;
  const owner = element.closest("input, textarea");
  if (owner instanceof HTMLInputElement && owner.type === "password") return true;
  return false;
}

/** 从节点向上找到块级祖先；找不到或到 body 返回 null。 */
export function findBlock(node: Node, doc: Document): Element | null {
  let element: Element | null =
    node.nodeType === Node.ELEMENT_NODE ? (node as Element) : node.parentElement;
  while (element) {
    if (element.matches(BLOCK_SELECTOR)) return element;
    if (element === doc.body || element === doc.documentElement) return null;
    element = element.parentElement;
  }
  return null;
}

interface TextPiece {
  node: Text;
  /** 折叠空白后的文本。 */
  text: string;
  /** 在本块折叠文本中的起始偏移。 */
  start: number;
}

interface BlockText {
  text: string;
  pieces: TextPiece[];
}

const HIDDEN_ANCESTOR = "script,style,noscript,template,[hidden],[aria-hidden='true']";

/** 遍历块内可见文本节点，折叠空白并记录每段偏移。 */
export function collectBlockText(block: Element): BlockText {
  const doc = block.ownerDocument;
  const view = doc.defaultView;
  const walker = doc.createTreeWalker(block, NodeFilter.SHOW_TEXT);
  const pieces: TextPiece[] = [];
  let text = "";
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const textNode = node as Text;
    const parent = textNode.parentElement;
    if (!parent || parent.closest(HIDDEN_ANCESTOR)) continue;
    if (view) {
      const style = view.getComputedStyle(parent);
      if (style.display === "none" || style.visibility === "hidden") continue;
    }
    const raw = textNode.nodeValue ?? "";
    if (raw.length === 0) continue;
    const collapsed = appendCollapsed(text, raw);
    if (collapsed.length === 0) continue;
    pieces.push({ node: textNode, text: collapsed, start: text.length });
    text += collapsed;
  }
  return { text, pieces };
}

/** 追加折叠后的文本：连续空白合成一个空格，开头不留空格。 */
function appendCollapsed(accumulated: string, raw: string): string {
  let appended = "";
  for (const char of raw) {
    if (/\s/.test(char)) {
      const current = accumulated + appended;
      if (current.length > 0 && !current.endsWith(" ")) appended += " ";
    } else {
      appended += char;
    }
  }
  return appended;
}

/** 选区起点在折叠块文本中的偏移。 */
function selectionOffset(pieces: TextPiece[], range: Range): number | null {
  const start = range.startContainer;
  for (const piece of pieces) {
    if (piece.node === start) {
      const nodeText = piece.node.nodeValue ?? "";
      const offset = Math.max(0, Math.min(range.startOffset, nodeText.length));
      if (nodeText.length > 0) {
        const prefix = piece.text.slice(0, clampLength(piece, offset));
        return piece.start + prefix.length;
      }
      return piece.start;
    }
  }
  // 选区起点在元素节点上（例如从节点间隙开始）：取该元素内第一个文本段。
  for (const piece of pieces) {
    if (start.contains(piece.node)) {
      return piece.start;
    }
  }
  return pieces.length > 0 ? pieces[0]?.start ?? 0 : null;
}

/**
 * 把节点内字符偏移换算成折叠后偏移。
 *
 * 折叠只影响空白，逐字符累计即可；避免对整段做 replace 后长度不一致。
 */
function clampLength(piece: TextPiece, offset: number): number {
  const nodeText = piece.node.nodeValue ?? "";
  let collapsed = "";
  for (let index = 0; index < offset && index < nodeText.length; index += 1) {
    const char = nodeText[index] ?? "";
    if (/\s/.test(char)) {
      if (collapsed.length > 0 && !collapsed.endsWith(" ")) collapsed += " ";
    } else {
      collapsed += char;
    }
  }
  return collapsed.length;
}

interface Segment {
  start: number;
  end: number;
}

/** 按句边界切分；优先 Intl.Segmenter，缺失时按标点回退。 */
export function segmentSentences(text: string): Segment[] {
  const SegmenterCtor = (
    Intl as unknown as {
      Segmenter?: new (
        locale: string | undefined,
        options: { granularity: "sentence" },
      ) => { segment: (input: string) => Iterable<{ index: number; segment: string }> };
    }
  ).Segmenter;
  if (typeof SegmenterCtor === "function") {
    const segmenter = new SegmenterCtor(undefined, { granularity: "sentence" });
    const segments: Segment[] = [];
    for (const segment of segmenter.segment(text)) {
      if (segment.segment.trim().length > 0) {
        segments.push({ start: segment.index, end: segment.index + segment.segment.length });
      }
    }
    if (segments.length > 0) return segments;
  }
  return punctuationSegments(text);
}

/** 标点回退：中英文句末标点与换行。 */
function punctuationSegments(text: string): Segment[] {
  const segments: Segment[] = [];
  let start = 0;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index] ?? "";
    if (/[.!?。！？\n]/.test(char)) {
      const end = index + 1;
      if (text.slice(start, end).trim().length > 0) {
        segments.push({ start, end });
      }
      start = end;
    }
  }
  if (start < text.length && text.slice(start).trim().length > 0) {
    segments.push({ start, end: text.length });
  }
  return segments;
}

/** 选择包含偏移的句段；没有则取距离最近的。 */
function nearestSegment(segments: Segment[], offset: number): Segment | null {
  let best: Segment | null = null;
  let bestDistance = Number.POSITIVE_INFINITY;
  for (const segment of segments) {
    if (segment.start <= offset && offset < segment.end) return segment;
    const distance =
      offset < segment.start ? segment.start - offset : offset - (segment.end - 1);
    if (distance < bestDistance) {
      best = segment;
      bestDistance = distance;
    }
  }
  return best;
}

/** 取离偏移最近的选词出现位置。 */
function nearestOccurrence(text: string, word: string, offset: number): number {
  let index = text.indexOf(word);
  let best = index;
  let bestDistance = Number.POSITIVE_INFINITY;
  while (index >= 0) {
    const distance = Math.abs(index - offset);
    if (distance < bestDistance) {
      best = index;
      bestDistance = distance;
    }
    index = text.indexOf(word, index + 1);
  }
  return best;
}

/** 截断到 600 字符，尽量保留选词两侧。 */
function clampSentence(sentence: string, word: string): string {
  const trimmed = sentence.trim();
  if (trimmed.length <= MAX_SENTENCE_CHARS) return trimmed;
  const index = Math.max(0, trimmed.indexOf(word));
  const start = Math.max(0, Math.min(index - 200, trimmed.length - MAX_SENTENCE_CHARS));
  return trimmed.slice(start, start + MAX_SENTENCE_CHARS).trim();
}

/** 从折叠块文本中取一句；块短时用整块。 */
export function pickSentence(blockText: string, offset: number, word: string): string {
  const text = blockText.trim();
  if (text.length === 0) return "";
  if (text.length <= WHOLE_BLOCK_CHARS) return clampSentence(text, word);

  const segments = segmentSentences(text);
  const segment = nearestSegment(segments, offset);
  let sentence = segment ? text.slice(segment.start, segment.end) : "";
  if (sentence.length === 0 || !sentence.includes(word)) {
    const index = nearestOccurrence(text, word, offset);
    if (index >= 0) {
      const start = Math.max(0, index - 160);
      const end = Math.min(text.length, index + word.length + 240);
      sentence = text.slice(start, end);
    }
  }
  if (sentence.length === 0) sentence = text;
  return clampSentence(sentence, word);
}

/** 由 Range 生成快照；不合格返回 null。 */
export function snapshotFromRange(range: Range, doc: Document): SentenceSnapshot | null {
  if (range.collapsed) return null;
  const word = normalizeWord(range.toString());
  if (!word) return null;
  if (isPasswordSelection(range)) return null;
  const block = findBlock(range.startContainer, doc);
  if (!block) return null;
  const { text, pieces } = collectBlockText(block);
  if (text.length === 0) return null;
  const offset = selectionOffset(pieces, range) ?? 0;
  const sentence = pickSentence(text, offset, word);
  if (!sentence.includes(word)) return null;
  return { word, sentence, block_offset: offset };
}

/** 读取当前窗口选区；不支持或不合格返回 null。 */
export function readSelectionSnapshot(doc: Document): SentenceSnapshot | null {
  const selection = doc.defaultView?.getSelection();
  if (!selection || selection.rangeCount === 0 || selection.isCollapsed) return null;
  const range = selection.getRangeAt(0);
  return snapshotFromRange(range, doc);
}
