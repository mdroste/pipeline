import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import { validateReleaseIdentity } from "./validate-release-identity.mjs";

function fixture(overrides = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-release-identity-"));
  const tauri = path.join(root, "gui", "src-tauri");
  fs.mkdirSync(tauri, { recursive: true });
  fs.mkdirSync(path.join(root, "scripts", "release"), { recursive: true });
  const version = "2.3.4";
  fs.writeFileSync(path.join(root, "gui", "package.json"), JSON.stringify({ version }));
  fs.writeFileSync(
    path.join(root, "gui", "package-lock.json"),
    JSON.stringify({ version, packages: { "": { version } }, ...overrides.packageLock }),
  );
  fs.writeFileSync(path.join(tauri, "Cargo.toml"), `[package]\nname = "pipeline-gui"\nversion = "${overrides.cargo ?? version}"\n`);
  fs.writeFileSync(path.join(tauri, "Cargo.lock"), `[[package]]\nname = "pipeline-gui"\nversion = "${overrides.cargoLock ?? version}"\n`);
  fs.writeFileSync(
    path.join(tauri, "tauri.conf.json"),
    JSON.stringify({
      version: overrides.tauri ?? "../package.json",
      bundle: {
        macOS: { minimumSystemVersion: overrides.minimumSystemVersion ?? "15.0" },
        resources: overrides.resources ?? ["resources/poppler/**/*", "resources/notices/**/*"],
      },
    }),
  );
  fs.writeFileSync(
    path.join(root, "scripts", "release", "poppler-lock.json"),
    JSON.stringify({ macos: { deploymentTarget: overrides.deploymentTarget ?? "15.0" } }),
  );
  return root;
}

test("accepts the canonical package version everywhere and an exact tag", () => {
  const root = fixture();
  assert.equal(validateReleaseIdentity({ rootDir: root, tag: "v2.3.4" }), "2.3.4");
});

test("rejects a Cargo version mismatch", () => {
  const root = fixture({ cargo: "2.3.5" });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /Cargo\.toml.*2\.3\.5/);
});

test("rejects a tag mismatch", () => {
  const root = fixture();
  assert.throws(() => validateReleaseIdentity({ rootDir: root, tag: "v2.3.5" }), /must exactly equal v2\.3\.4/);
});

test("requires Tauri to read the canonical package.json", () => {
  const root = fixture({ tauri: "2.3.4" });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /must be "\.\.\/package\.json"/);
});

test("requires the declared macOS minimum to match the Poppler closure", () => {
  const root = fixture({ minimumSystemVersion: "14.0" });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /macOS minimum.*Poppler closure target/);
});

test("requires both generated release resource trees", () => {
  const root = fixture({ resources: ["resources/poppler/**/*"] });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /resources\/notices/);
});
