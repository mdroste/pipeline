#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, isAbsolute, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const explicit = process.argv.slice(2);
const discovered = execFileSync(
  "rg",
  [
    "--files",
    // Community files such as CONTRIBUTING.md live in .github/.
    "--hidden",
    "-g",
    "!.git",
    "-g",
    "*.md",
    "-g",
    "!gui/src-tauri/codex-protocol/**",
  ],
  { cwd: repository, encoding: "utf8" },
)
  .split(/\r?\n/)
  .filter(Boolean);
// The agent guides are local-only (gitignored), so they are absent from a clean checkout.
const localGuides = ["CLAUDE.md", "AGENTS.md"];
const files = explicit.length
  ? explicit
  : [...discovered, ...localGuides.filter((file) => existsSync(resolve(repository, file)))];
const failures = [];

// GitHub heading anchors: lowercase, punctuation removed, spaces as hyphens,
// and a numeric suffix for repeated headings.
const anchorCache = new Map();
function anchors(path) {
  if (!anchorCache.has(path)) {
    const seen = new Map();
    const found = new Set();
    let fenced = false;
    for (const line of readFileSync(path, "utf8").split(/\r?\n/)) {
      if (/^\s*(```|~~~)/.test(line)) fenced = !fenced;
      if (fenced) continue;
      for (const id of line.matchAll(/<a\s+(?:name|id)="([^"]+)"/g)) found.add(id[1]);
      const heading = line.match(/^#{1,6}\s+(.*?)\s*#*\s*$/);
      if (!heading) continue;
      const text = heading[1].replace(/\[([^\]]*)\]\([^)]*\)/g, "$1").replace(/[`*]/g, "");
      const base = text.toLowerCase().replace(/[^\p{L}\p{N}\p{M} _-]/gu, "").replace(/ /g, "-");
      const count = seen.get(base) ?? 0;
      seen.set(base, count + 1);
      found.add(count ? `${base}-${count}` : base);
    }
    anchorCache.set(path, found);
  }
  return anchorCache.get(path);
}

for (const file of new Set(files)) {
  const absolute = resolve(repository, file);
  if (!existsSync(absolute) || !statSync(absolute).isFile()) {
    failures.push(`${file}: file does not exist`);
    continue;
  }
  const markdown = readFileSync(absolute, "utf8");
  const links = /\[[^\]]*\]\(([^)]+)\)/g;
  let match;
  while ((match = links.exec(markdown))) {
    const line = markdown.slice(0, match.index).split(/\r?\n/).length;
    let target = match[1].trim().replace(/^<|>$/g, "");
    if (target.startsWith("#")) {
      if (!anchors(absolute).has(decodeURIComponent(target.slice(1)))) {
        failures.push(`${file}:${line}: missing heading ${match[1]}`);
      }
      continue;
    }
    if (!target || /^(?:https?:|mailto:|codex:)/.test(target)) continue;
    const fragment = target.includes("#") ? target.slice(target.indexOf("#") + 1) : "";
    target = target.split("#", 1)[0];
    try {
      target = decodeURIComponent(target);
    } catch {
      failures.push(`${file}:${line}: invalid URL encoding in ${match[1]}`);
      continue;
    }
    // Codex file links may carry a one-based line suffix.
    target = target.replace(/:\d+$/, "");
    // Absolute filesystem paths name one machine, not a location in the repository.
    if (isAbsolute(target)) continue;
    const resolved = resolve(dirname(absolute), target);
    if (localGuides.includes(relative(repository, resolved))) continue;
    if (!existsSync(resolved)) {
      failures.push(`${file}:${line}: missing ${match[1]}`);
    } else if (fragment && resolved.endsWith(".md") && !anchors(resolved).has(decodeURIComponent(fragment))) {
      failures.push(`${file}:${line}: missing heading ${match[1]}`);
    }
  }
}

if (failures.length) {
  console.error("Broken local Markdown links:");
  for (const failure of failures) console.error(`  ${failure}`);
  process.exitCode = 1;
} else {
  console.log(`Markdown links OK (${files.length} files checked).`);
}
