import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./check-doc-style.mjs", import.meta.url));

function checkChangelog(t, text) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-docs-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.writeFileSync(path.join(root, "CHANGELOG.md"), text);
  return spawnSync(process.execPath, [script, `--root=${root}`, "CHANGELOG.md"], { encoding: "utf8" });
}

test("a style error in the newest changelog section fails when there is no Unreleased section", (t) => {
  const result = checkChangelog(t, "# Changelog\n\n## 1.2.0 — 2026-01-02\n\n- Finished WB-12.\n\n## 1.1.0 — 2026-01-01\n\n- Done.\n");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /CHANGELOG\.md:5: internal milestone or audit code/);
});

test("the newest version heading may carry its release date", (t) => {
  const result = checkChangelog(t, "# Changelog\n\n## 1.2.0 — 2026-01-02\n\n- Fine.\n");
  assert.equal(result.status, 0, result.stderr);
});

test("released changelog sections are history and stay unchecked", (t) => {
  const result = checkChangelog(t, "# Changelog\n\n## 1.2.0 — 2026-01-02\n\n- Fine.\n\n## 1.1.0 — 2026-01-01\n\n- Finished WB-12.\n");
  assert.equal(result.status, 0, result.stderr);
});

test("an Unreleased section is checked instead of the newest version", (t) => {
  const result = checkChangelog(t, "# Changelog\n\n## Unreleased\n\n- Finished WB-12.\n\n## 1.2.0 — 2026-01-02\n\n- Fine.\n");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /CHANGELOG\.md:5:/);
});
