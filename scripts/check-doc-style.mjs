#!/usr/bin/env node
// Checks tracked Markdown against the writing rules in CONTRIBUTING.md.
// Errors fail the check; warnings print only with --warnings.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(fileURLToPath(import.meta.url), "../..");
const args = process.argv.slice(2);
const showWarnings = args.includes("--warnings");
// --root=<dir> names files relative to another folder, such as a draft copy of the repository.
const root = resolve(args.find((arg) => arg.startsWith("--root="))?.slice(7) ?? repository);
const explicit = args.filter((arg) => !arg.startsWith("--"));
const files = explicit.length
  ? explicit
  : execFileSync(
      "rg",
      [
        "--files",
        "-g",
        "*.md",
        "-g",
        "!prompts/**",
        "-g",
        "prompts/auto_review/catalog/README.md",
        "-g",
        "!.agents/**",
        "-g",
        "!docs/workbench/protocol/**",
        // Local agent guides are gitignored and not published.
        "-g",
        "!CLAUDE.md",
        "-g",
        "!AGENTS.md",
      ],
      { cwd: repository, encoding: "utf8" },
    )
      .split(/\r?\n/)
      .filter(Boolean);

// Researcher-facing pages use the app's words, not implementation terms.
const userPages = (file) =>
  ["README.md", "PRIVACY.md", "SUPPORT.md", "SECURITY.md", "examples/research-exporters/README.md"].includes(file) ||
  (file.startsWith("docs/") && file !== "docs/architecture.md");

const month = "(?:January|February|March|April|May|June|July|August|September|October|November|December)";
const rules = [
  {
    id: "dated-heading",
    test: (line) =>
      /^#{1,6}\s/.test(line) &&
      (new RegExp(`${month} \\d{1,2}`).test(line) || /\b20\d\d-\d\d-\d\d\b/.test(line) || /^#{1,6}\s+(Validation|Qualification)\b/i.test(line)),
    message: "dated or validation heading; test records belong in reports/",
  },
  {
    id: "internal-code",
    test: (line) => /\bWB-\d+\b|\bR\d\s*[–-]\s*R\d\b|\bcatalog \d+\b|\bfindings? \d+\s*[–-]\s*\d+\b/i.test(line),
    message: "internal milestone or audit code",
  },
  {
    id: "reports-link",
    test: (line) => /\]\((?:\.\.\/)*reports\//.test(line),
    message: "link into the local reports/ folder",
  },
];
const vocabulary =
  /\b(explicit(?:ly)?|bounded|durabl[ey]|immutable|authoritative|provenance|invariants?|harness(?:es)?|workbench|fails? closed|epoch|DTOs?|facades?)\b/i;

const errors = [];
const warnings = [];
for (const file of new Set(files)) {
  const name = relative(root, resolve(root, file)).split(sep).join("/");
  let lines = readFileSync(resolve(root, file), "utf8").split(/\r?\n/);
  let offset = 0;
  if (name === "CHANGELOG.md") {
    // Released entries are history; only the Unreleased section is checked.
    const start = lines.findIndex((line) => /^## Unreleased/.test(line));
    const end = lines.findIndex((line, index) => index > start && /^## /.test(line));
    offset = Math.max(start, 0);
    lines = start < 0 ? [] : lines.slice(start, end < 0 ? undefined : end);
  }
  let fenced = false;
  let paragraph = [];
  const flush = (endLine) => {
    const text = paragraph.join(" ");
    if (!text) return;
    const startLine = endLine - paragraph.length + 1;
    if ((text.match(/;/g) ?? []).length > 2) warnings.push(`${name}:${startLine}: more than two semicolons in a paragraph`);
    for (const sentence of text.split(/(?<=[.!?])\s+/)) {
      const words = sentence.split(/\s+/).filter(Boolean).length;
      if (words > 35) warnings.push(`${name}:${startLine}: ${words}-word sentence`);
    }
    paragraph = [];
  };
  lines.forEach((raw, index) => {
    const lineNumber = offset + index + 1;
    if (/^\s*(```|~~~)/.test(raw)) {
      fenced = !fenced;
      flush(lineNumber - 1);
      return;
    }
    if (fenced) return;
    // Code spans and link targets may name paths and identifiers.
    const line = raw.replace(/`[^`]*`/g, "``").replace(/\]\([^)]*\)/g, "]()");
    for (const rule of rules) {
      if (rule.test(rule.id === "reports-link" ? raw : line)) errors.push(`${name}:${lineNumber}: ${rule.message}`);
    }
    if (userPages(name)) {
      const match = line.match(vocabulary);
      if (match) errors.push(`${name}:${lineNumber}: "${match[0]}" is implementation vocabulary in a user guide`);
    }
    if (!raw.trim() || /^\s*(\||#|[-*+] |\d+\. |>)/.test(raw)) {
      flush(lineNumber - 1);
      if (/^\s*([-*+] |\d+\. |>)/.test(raw)) paragraph.push(line.replace(/^\s*([-*+]|\d+\.|>)\s*/, ""));
      return;
    }
    paragraph.push(line.trim());
  });
  flush(offset + lines.length);
}

for (const error of errors) console.error(`  ${error}`);
if (showWarnings) for (const warning of warnings) console.warn(`  warning: ${warning}`);
const summary = `${files.length} files, ${errors.length} errors, ${warnings.length} warnings`;
if (errors.length) {
  console.error(`Documentation style check failed (${summary}).`);
  process.exitCode = 1;
} else {
  console.log(`Documentation style OK (${summary}${showWarnings || !warnings.length ? "" : "; run with --warnings to list them"}).`);
}
