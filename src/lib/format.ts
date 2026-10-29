/**
 * 展示层格式化辅助。保持纯函数，便于单测与其他窗口复用。
 * 文案遵循 docs/FRONTEND_GUIDELINES.md §9：简短、不显示原始异常。
 */

function pad2(value: number): string {
  return value < 10 ? `0${value}` : `${value}`;
}

/**
 * 查询耗时。小于 1s 用毫秒，否则保留一位小数的秒。
 * 非有限值或负数返回占位符“—”。
 */
export function formatElapsedMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  return `${(ms / 1000).toFixed(1)} s`;
}

/** 词条条数，例如 “3 条”。非有限值返回“—”。 */
export function formatEntryCount(count: number): string {
  if (!Number.isFinite(count)) return "—";
  return `${Math.max(0, Math.round(count))} 条`;
}

/** 本地时间 `YYYY-MM-DD HH:mm`；非有限值或非法时间返回“—”。 */
export function formatTimestamp(ms: number): string {
  if (!Number.isFinite(ms)) return "—";
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "—";
  return (
    `${date.getFullYear()}-${pad2(date.getMonth() + 1)}-${pad2(date.getDate())} ` +
    `${pad2(date.getHours())}:${pad2(date.getMinutes())}`
  );
}

/**
 * 按字符（而非 UTF-16 码元）截断，超出时用省略号占位。
 * `max <= 0` 返回空串；`max` 为 1 时只返回省略号。
 */
export function truncate(text: string, max: number): string {
  const limit = Math.floor(max);
  if (!Number.isFinite(limit) || limit <= 0) return "";
  const characters = Array.from(text);
  if (characters.length <= limit) return text;
  return `${characters.slice(0, limit - 1).join("")}…`;
}
