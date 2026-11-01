import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./source-size-report.mjs", import.meta.url));
const { maxLines } = JSON.parse(
  fs.readFileSync(new URL("./source-size-policy.json", import.meta.url), "utf8"),
);

// Writes one source file per entry, then runs --check against that tree alone.
function check(t, sources, baselineFiles) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-size-"));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [name, lines] of Object.entries(sources)) {
    fs.writeFileSync(path.join(root, name), "x\n".repeat(lines));
  }
  const manifest = path.join(root, "manifest.txt");
  fs.writeFileSync(manifest, Object.keys(sources).join("\n"));
  const baseline = path.join(root, "baseline.json");
  fs.writeFileSync(baseline, JSON.stringify({ files: baselineFiles }));
  return spawnSync(
    process.execPath,
    [script, "--check", "--json", "--root", root, "--manifest", manifest, "--baseline", baseline],
    { encoding: "utf8" },
  );
}
const stale = (result) => JSON.parse(result.stdout).staleEntries;

test("a baseline entry for a file still over budget passes while the file does not grow", (t) => {
  const result = check(t, { "large.rs": maxLines + 1 }, { "large.rs": [maxLines + 5, 1_000_000] });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(stale(result), []);
});

test("a baseline entry for a file now within budget fails the check", (t) => {
  const result = check(t, { "small.rs": 10 }, { "small.rs": [maxLines + 5, 1_000_000] });
  assert.equal(result.status, 1);
  assert.deepEqual(stale(result), ["small.rs"]);
});

test("a baseline entry for a deleted file fails the check", (t) => {
  const result = check(t, { "kept.rs": 10 }, { "deleted.rs": [maxLines + 5, 1_000_000] });
  assert.equal(result.status, 1);
  assert.deepEqual(stale(result), ["deleted.rs"]);
});

test("a file that grows past its entry still fails", (t) => {
  const result = check(t, { "large.rs": maxLines + 9 }, { "large.rs": [maxLines + 5, 1_000_000] });
  assert.equal(result.status, 1);
  assert.deepEqual(stale(result), []);
});
