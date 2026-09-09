#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const explicit = process.argv.slice(2);
const discovered = execFileSync(
  "rg",
  [
    "--files",
    "-g",
    "*.md",
    "-g",
    "!notes/**",
    "-g",
    "!development/**",
    "-g",
    "!docs/workbench/protocol/**",
  ],
  { cwd: repository, encoding: "utf8" },
)
  .split(/\r?\n/)
  .filter(Boolean);
const files = explicit.length ? explicit : [...discovered, "CLAUDE.md", "AGENTS.md"];
const failures = [];

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
    if (!target || /^(?:https?:|mailto:|codex:|#)/.test(target)) continue;
    target = target.split("#", 1)[0];
    try {
      target = decodeURIComponent(target);
    } catch {
      failures.push(`${file}:${line}: invalid URL encoding in ${match[1]}`);
      continue;
    }
    // Codex file links may carry a one-based line suffix.
    target = target.replace(/:\d+$/, "");
    const resolved = resolve(dirname(absolute), target);
    if (!existsSync(resolved)) {
      failures.push(`${file}:${line}: missing ${match[1]}`);
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
