#!/usr/bin/env node
// Inventory mode is read-only; --check enforces only regressions beyond the baseline.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, lstatSync } from "node:fs";
import { dirname, extname, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const options = {};
for (let i = 0; i < args.length; i++) {
  const flag = args[i];
  if (flag === "--json" || flag === "--help" || flag === "--check") options[flag.slice(2)] = true;
  else if (["--root", "--manifest", "--baseline", "--write-baseline"].includes(flag)) {
    if (!args[i + 1] || args[i + 1].startsWith("--")) throw new Error(`${flag} needs a path`);
    options[flag.slice(2)] = args[++i];
  } else throw new Error(`Unknown argument: ${flag}`);
}
if (options.help) {
  console.log("Usage: node scripts/source-size-report.mjs [--json] [--check] [--baseline FILE] [--write-baseline FILE] [--manifest FILE] [--root DIR]");
  console.log("A manifest contains one repository-relative path per line. Default discovery uses rg; Git is never invoked.");
  console.log("--check fails only for newly oversized files or growth beyond the recorded baseline.");
  process.exit(0);
}

const root = resolve(options.root ?? repository);
const policy = JSON.parse(readFileSync(resolve(repository, "scripts/source-size-policy.json"), "utf8"));
const baselinePath = resolve(options.baseline ?? resolve(repository, "docs/refactoring/source-size-baseline.json"));
let baseline = { files: {} };
try { baseline = JSON.parse(readFileSync(baselinePath, "utf8")); }
catch (error) { if (error.code !== "ENOENT" || options.baseline) throw error; }
const extensions = new Set([".rs", ".ts", ".tsx", ".js", ".mjs", ".cjs", ".py", ".css", ".sql", ".sh", ".html", ".md", ".json", ".toml", ".yml", ".yaml"]);
const paths = options.manifest
  ? readFileSync(resolve(options.manifest), "utf8").split(/\r?\n/)
  : execFileSync("rg", ["--files", "--hidden", "-g", "!.git", "-g", "!node_modules", "-g", "!target", "-g", "!dist"], { cwd: root, encoding: "utf8" }).split(/\r?\n/);
// These local navigation files are intentionally ignored in this repository.
if (!options.manifest) paths.push("CLAUDE.md", "AGENTS.md");

function kindOf(path) {
  if (policy.generated.some((entry) => entry.path === path)) return "generated";
  if (path.endsWith(".md")) return "document";
  if (/\.(json|toml|ya?ml)$/.test(path) || path.endsWith("Cargo.lock")) return "data/config";
  if (/(\.test\.|\/tests(?:\/|\.)|\/e2e\/)/.test(path)) return "test";
  return "source";
}

const files = [];
for (const path of new Set(paths.filter(Boolean))) {
  const absolute = resolve(root, path);
  const normalized = relative(root, absolute).replaceAll("\\", "/");
  if (normalized.startsWith("../") || normalized === "..") throw new Error(`Path outside root: ${path}`);
  if (!extensions.has(extname(path)) && !path.endsWith("Cargo.lock")) continue;
  let stat;
  try { stat = lstatSync(absolute); }
  catch (error) { if (error.code === "ENOENT") continue; throw error; }
  if (!stat.isFile()) continue;
  const bytes = readFileSync(absolute);
  const text = bytes.toString("utf8");
  const lines = text.length ? text.replace(/\r?\n$/, "").split(/\r?\n/) : [];
  const kind = kindOf(normalized);
  const owned = kind === "source" || kind === "test";
  const overBudget = owned && (lines.length > policy.maxLines || bytes.length > policy.maxBytes);
  const previous = baseline.files[normalized];
  files.push({
    path: normalized, kind, lines: lines.length, bytes: bytes.length,
    longLines: lines.filter((line) => line.length > policy.longLineCharacters).length,
    overBudget,
    change: !overBudget ? "within" : !previous ? "new" :
      lines.length > previous[0] || bytes.length > previous[1] ? "grew" : "baseline",
  });
}
files.sort((a, b) => b.lines - a.lines || a.path.localeCompare(b.path));

if (options["write-baseline"]) {
  const selected = files.filter((file) => file.overBudget).sort((a, b) => a.path.localeCompare(b.path));
  // Compact generated measurements: one readable file entry per line.
  const body = selected.map((file) => `    ${JSON.stringify(file.path)}: [${file.lines}, ${file.bytes}]`).join(",\n");
  writeFileSync(resolve(options["write-baseline"]), `{
  "description": "Current accepted source/test size debt; values are [physical lines, UTF-8 bytes]. Do not refresh merely to hide growth.",
  "files": {
${body}
  }
}\n`);
}

const warnings = files.filter((file) => file.overBudget);
const report = {
  policy: {
    maxLines: policy.maxLines,
    maxBytes: policy.maxBytes,
    reportingOnly: !options.check,
  },
  summary: {
    files: files.length,
    overBudget: warnings.length,
    newOrGrowing: warnings.filter((file) => file.change === "new" || file.change === "grew").length,
  },
  files,
};
if (options.json) console.log(JSON.stringify(report, null, 2));
else {
  console.log(`Source size report: ${files.length} files; ${warnings.length} source/test files over ${policy.maxLines} lines or ${policy.maxBytes} bytes.`);
  console.log(
    options.check
      ? "Enforcing newly oversized files and growth beyond the baseline."
      : "Reporting only. Generated data and documents do not fail the source budget.",
  );
  for (const file of warnings) console.log(`${String(file.lines).padStart(5)} lines ${String(file.bytes).padStart(7)} bytes ${file.change.padEnd(8)} ${file.path}`);
  const excluded = files.filter((file) => !["source", "test"].includes(file.kind) && file.lines > policy.maxLines);
  if (excluded.length) console.log(`Excluded oversized documents/data: ${excluded.map((file) => `${file.path} (${file.kind})`).join(", ")}`);
}

if (options.check) {
  const regressions = warnings.filter(
    (file) => file.change === "new" || file.change === "grew",
  );
  if (regressions.length) {
    if (options.json) {
      console.error(
        `Source-size check failed: ${regressions.length} newly oversized or growing file(s).`,
      );
    }
    process.exitCode = 1;
  }
}
