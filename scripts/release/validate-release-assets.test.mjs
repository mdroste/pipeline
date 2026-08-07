import assert from "node:assert/strict";
import test from "node:test";

import {
  expectedReleaseAssets,
  validateReleaseAssets,
} from "./validate-release-assets.mjs";

test("requires installer, checksum, artifact/build-input SBOMs, and provenance for every platform", () => {
  const expected = expectedReleaseAssets("2.3.4");
  assert.equal(expected.length, 20);
  assert.ok(expected.includes("Pipeline-2.3.4-Windows.exe.sha256"));
  assert.ok(expected.includes("Pipeline-2.3.4-macOS-Intel-Artifact-SBOM.cdx.json"));
  assert.ok(expected.includes("Pipeline-2.3.4-macOS-Intel-Build-input-SBOM.cdx.json"));
  assert.deepEqual(
    validateReleaseAssets({ version: "2.3.4", assetNames: expected }),
    expected,
  );
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
