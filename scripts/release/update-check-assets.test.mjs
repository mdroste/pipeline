import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { installerNames } from "./draft-release.mjs";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("update checks link to the installer names releases publish", () => {
  const source = fs.readFileSync(
    path.join(ROOT, "gui", "src-tauri", "src", "updates", "github.rs"),
    "utf8",
  );
  const block = source.match(/const INSTALLER_SUFFIX[\s\S]*?\n};/);
  assert.ok(block, "INSTALLER_SUFFIX must be defined in updates/github.rs");
  const suffixes = [...block[0].matchAll(/Some\("([^"]+)"\)/g)].map(([, suffix]) => suffix);
  assert.deepEqual(
    suffixes.map((suffix) => `Pipeline-2.3.4${suffix}`).sort(),
    installerNames("2.3.4").sort(),
  );
});
