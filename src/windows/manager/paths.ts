/** Windows path helpers shared by the manager windows. */

/** Last path segment; keeps Windows and POSIX separators working. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] ?? path;
}

/** Case-insensitive comparison key for Windows paths. */
export function normalisePath(path: string): string {
  return path.replace(/\//g, "\\").toLowerCase();
}

/** Human-readable file name without the `.mdx` extension. */
export function stripMdxExtension(name: string): string {
  return name.replace(/\.mdx$/i, "");
}

/** Compact summary for a source row: "3 个文件 · A.mdx、B.mdx …". */
export function summarisePaths(paths: string[]): string {
  const names = paths.map(basename);
  if (names.length <= 2) return names.join("、");
  return `${names.length} 个文件 · ${names.slice(0, 2).join("、")} …`;
}
