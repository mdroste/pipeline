import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { cleanBuildArtifacts } from "./clean-build-artifacts.mjs";

function fixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-clean-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const write = (relative) => {
    const file = path.join(root, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, "preserve or regenerate");
    return file;
  };
  return { root, write };
}
const quiet = { log: () => {} };

test("preview is read-only; apply removes outputs but preserves sources, dependencies, fixtures and local data", (t) => {
  const { root, write } = fixture(t);
  const output = write("gui/src-tauri/target/debug/incremental/query-cache.bin");
  const cache = write("gui/node_modules/.vite/results.json");
  const buildInfo = ["gui/tsconfig.tsbuildinfo", "gui/tsconfig.node.tsbuildinfo"].map(write);
  const keep = ["gui/src/main.tsx", "gui/test.pdf", "gui/node_modules/vite/package.json",
    ".env", ".claude/settings.local.json", ".git/config", "gui/src-tauri/resources/poppler/pdfinfo",
    "docs/untracked-notes.md", "gui/src-tauri/gen/keep.json"].map(write);
  cleanBuildArtifacts(root, quiet);
  assert.ok(fs.existsSync(output));
  assert.ok(fs.existsSync(cache));
  cleanBuildArtifacts(root, { ...quiet, apply: true });
  assert.ok(!fs.existsSync(output));
  assert.ok(!fs.existsSync(cache));
  for (const file of buildInfo) assert.ok(!fs.existsSync(file), file);
  for (const file of keep) assert.ok(fs.existsSync(file), file);
  assert.deepEqual(cleanBuildArtifacts(root, { ...quiet, apply: true }), []);
});

test("nested worktrees require opt-in and retain their source changes", (t) => {
  const { root, write } = fixture(t);
  const prefix = ".claude/worktrees/example/";
  const marker = write(`${prefix}.git`);
  const source = write(`${prefix}gui/src/changed.ts`);
  const output = write(`${prefix}gui/src-tauri/target/debug/cache`);
  const unrelated = write(".claude/worktrees/not-a-checkout/gui/dist/keep");
  cleanBuildArtifacts(root, { ...quiet, apply: true });
  assert.ok(fs.existsSync(output));
  cleanBuildArtifacts(root, { ...quiet, apply: true, worktrees: true });
  assert.ok(!fs.existsSync(output));
  for (const file of [marker, source, unrelated]) assert.ok(fs.existsSync(file));
});

test("a symlink in an output path aborts the entire plan before deletion", (t) => {
  const { root, write } = fixture(t);
  const output = write("gui/src-tauri/target/debug/cache");
  const external = write("external/.vite/keep");
  fs.symlinkSync(path.join(root, "external"), path.join(root, "gui/node_modules"), "junction");
  assert.throws(() => cleanBuildArtifacts(root, { ...quiet, apply: true }), /symlink/);
  assert.ok(fs.existsSync(output));
  assert.ok(fs.existsSync(external));
});

test("symlinks inside disposable outputs do not delete their destinations", (t) => {
  const { root, write } = fixture(t);
  const external = write("external/keep");
  write("gui/dist/index.html");
  fs.symlinkSync(path.join(root, "external"), path.join(root, "gui/dist/link"), "junction");
  cleanBuildArtifacts(root, { ...quiet, apply: true });
  assert.ok(fs.existsSync(external));
  assert.ok(!fs.existsSync(path.join(root, "gui/dist")));
});
