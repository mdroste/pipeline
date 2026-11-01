import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  expectedReleaseAssets,
  validateReleaseAssets,
} from "./validate-release-assets.mjs";

test("requires what a release publishes: installers, update packages, checksums, signatures, and the manifest", () => {
  const expected = expectedReleaseAssets("2.3.4");
  assert.equal(expected.length, 17);
  for (const name of [
    "Pipeline-2.3.4-Windows.exe",
    "Pipeline-2.3.4-Windows.exe.sha256",
    "Pipeline-2.3.4-Windows.exe.sig",
    "Pipeline-2.3.4-macOS-Intel.dmg.sha256",
    "Pipeline-2.3.4-macOS-Intel.app.tar.gz.sha256",
    "Pipeline-2.3.4-macOS-AppleSilicon.app.tar.gz.sig",
    "latest.json",
  ]) assert.ok(expected.includes(name), name);
  assert.ok(!expected.some((name) => /SBOM|provenance/i.test(name)));
  assert.deepEqual(
    validateReleaseAssets({ version: "2.3.4", assetNames: expected }),
    expected,
  );
});

test("the command checks a draft's asset list as gh reports it", () => {
  const expected = expectedReleaseAssets("2.3.4");
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-release-assets-"));
  const run = (release) => {
    const file = path.join(folder, "release.json");
    fs.writeFileSync(file, JSON.stringify(release));
    return spawnSync(process.execPath, [
      fileURLToPath(new URL("./validate-release-assets.mjs", import.meta.url)),
      "--version", "2.3.4", "--release-json", file,
    ], { encoding: "utf8" });
  };
  try {
    const assets = expected.map((name) => ({ name }));
    assert.equal(run({ isDraft: true, assets }).status, 0);
    assert.match(run({ isDraft: false, assets }).stderr, /must run against the draft/);
    assert.match(run({ isDraft: true, assets: assets.slice(1) }).stderr, /missing required assets/);
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
});

test("rejects an incomplete release inventory", () => {
  const expected = expectedReleaseAssets("2.3.4");
  const missing = "Pipeline-2.3.4-Linux.AppImage.sha256";
  assert.throws(
    () => validateReleaseAssets({
      version: "2.3.4",
      assetNames: expected.filter((name) => name !== missing),
    }),
    new RegExp(missing.replaceAll(".", "\\.")),
  );
});

test("rejects prerelease versions until the publishing channel is explicit", () => {
  assert.throws(() => expectedReleaseAssets("2.3.4-rc.1"), /stable SemVer/);
});

test("rejects unexpected updater or stale assets", () => {
  const expected = expectedReleaseAssets("2.3.4");
  assert.throws(
    () => validateReleaseAssets({
      version: "2.3.4",
      assetNames: [...expected, "Pipeline_x64.app.tar.gz"],
    }),
    /unexpected assets.*app\.tar\.gz/,
  );
});
