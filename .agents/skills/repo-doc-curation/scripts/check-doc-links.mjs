#!/usr/bin/env node
// Check relative Markdown links and the AGENTS.md line budget.
// Usage: node check-doc-links.mjs [repo-root]
import fs from "node:fs";
import path from "node:path";

const root = path.resolve(process.argv[2] ?? process.cwd());
const SKIP_DIRS = new Set([".git", "node_modules", "target", "dist", ".output", ".wxt", "artifacts", "references"]);
const LINK_RE = /\[([^\]]*)\]\(([^)]+)\)/g;

function walk(dir, out = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".") && entry.name !== ".") {
      if (entry.isDirectory() && SKIP_DIRS.has(entry.name)) continue;
    }
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      walk(full, out);
    } else if (entry.name.endsWith(".md")) {
      out.push(full);
    }
  }
  return out;
}

function stripCode(text) {
  let fenced = false;
  return text
    .split(/\r?\n/)
    .filter((line) => {
      if (/^\s*```/.test(line)) { fenced = !fenced; return false; }
      return !fenced;
    })
    .join("\n")
    .replace(/`[^`]*`/g, "");
}

let missing = 0;
let checked = 0;
for (const file of walk(root)) {
  const source = stripCode(fs.readFileSync(file, "utf8"));
  for (const match of source.matchAll(LINK_RE)) {
    let target = match[2].trim();
    if (target.startsWith("<") && target.endsWith(">")) target = target.slice(1, -1);
    target = target.split("#")[0].split("?")[0].trim();
    if (!target || /^(https?:|mailto:|data:|#|\/)/i.test(target)) continue;
    let decoded = target;
    try { decoded = decodeURIComponent(target); } catch { /* keep raw */ }
    const resolved = path.resolve(path.dirname(file), decoded);
    checked += 1;
    if (!fs.existsSync(resolved)) {
      missing += 1;
      console.log(`MISSING ${path.relative(root, file)} -> ${match[2].trim()}`);
    }
  }
}

let budgetFailed = false;
const agents = path.join(root, "AGENTS.md");
if (fs.existsSync(agents)) {
  const lines = fs.readFileSync(agents, "utf8").split(/\r?\n/).length;
  console.log(`AGENTS.md lines: ${lines}/60`);
  if (lines > 60) {
    budgetFailed = true;
    console.log("BUDGET AGENTS.md exceeds 60 lines");
  }
}

console.log(`Checked ${checked} relative links in ${walk(root).length} Markdown files.`);
if (missing > 0 || budgetFailed) process.exit(1);