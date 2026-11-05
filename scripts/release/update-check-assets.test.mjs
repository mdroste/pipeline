import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { installerNames } from "./draft-release.mjs";
import { packageUrl, updatePackages } from "./update-manifest.mjs";

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

test("in-app updates accept the packages releases publish", () => {
  const source = fs.readFileSync(
    path.join(ROOT, "gui", "src-tauri", "src", "updates", "github.rs"),
    "utf8",
  );
  const installers = source.match(/const INSTALLER_SUFFIX[\s\S]*?\n};/)[0];
  const packages = source.match(/const PACKAGE_SUFFIX[\s\S]*?\n};/);
  assert.ok(packages, "PACKAGE_SUFFIX must be defined in updates/github.rs");
  const suffixes = [...packages[0].matchAll(/Some\("([^"]+)"\)/g)].map(([, suffix]) => suffix);
  // Platforms without their own package fall through to the installer.
  assert.match(packages[0], /INSTALLER_SUFFIX/);
  const nonMac = [...installers.matchAll(/Some\("([^"]+)"\)/g)]
    .map(([, suffix]) => suffix)
    .filter((suffix) => !suffix.includes("macOS"));
  assert.deepEqual(
    [...suffixes, ...nonMac].map((suffix) => `Pipeline-2.3.4${suffix}`).sort(),
    updatePackages("2.3.4").map(({ name }) => name).sort(),
  );

  const owner = source.match(/const OWNER: &str = "([^"]+)"/)[1];
  const repo = source.match(/const REPO: &str = "([^"]+)"/)[1];
  assert.equal(
    packageUrl("2.3.4", "file"),
    `https://github.com/${owner}/${repo}/releases/download/v2.3.4/file`,
  );
});
