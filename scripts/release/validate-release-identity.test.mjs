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
  const version = overrides.version ?? "2.3.4";
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
      $schema: overrides.schema ?? "https://schema.tauri.app/config/2",
      version: overrides.tauri ?? "../package.json",
      identifier: overrides.identifier ?? "com.pipeline.report",
      app: {
        windows: [{
          minWidth: overrides.minWidth ?? 1024,
          minHeight: overrides.minHeight ?? 700,
        }],
      },
      bundle: {
        macOS: { minimumSystemVersion: overrides.minimumSystemVersion ?? "15.0" },
        windows: {
          allowDowngrades: overrides.allowDowngrades ?? false,
          webviewInstallMode: overrides.webviewInstallMode ?? {
            type: "offlineInstaller",
            silent: true,
          },
        },
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

test("rejects prerelease and build-metadata tags from the stable release workflow", () => {
  for (const version of ["2.3.4-rc.1", "2.3.4+rebuild.1"]) {
    const root = fixture({ version });
    assert.throws(
      () => validateReleaseIdentity({ rootDir: root, tag: `v${version}` }),
      /publishes stable releases only/,
    );
  }
});

test("generic quality validation remains valid on a tagged checkout", () => {
  const root = fixture();
  assert.equal(
    validateReleaseIdentity({ rootDir: root, existingTags: ["v2.3.4"] }),
    "2.3.4",
  );
});

test("explicit release validation rejects a stable version older than an existing stable tag", () => {
  const root = fixture();
  assert.throws(
    () => validateReleaseIdentity({
      rootDir: root,
      tag: "v2.3.4",
      existingTags: ["v2.4.0"],
    }),
    /must be newer than existing stable tag v2\.4\.0/,
  );
});

test("accepts the exact new tag when it is newer than prior stable tags", () => {
  const root = fixture();
  assert.equal(
    validateReleaseIdentity({
      rootDir: root,
      tag: "v2.3.4",
      existingTags: ["v2.3.3", "v2.3.4"],
    }),
    "2.3.4",
  );
});

test("requires Tauri to read the canonical package.json", () => {
  const root = fixture({ tauri: "2.3.4" });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /must be "\.\.\/package\.json"/);
});

test("rejects a mutable development-branch Tauri schema", () => {
  const root = fixture({
    schema: "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/schema.json",
  });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /stable v2 endpoint/);
});

test("preserves the stable application identifier", () => {
  const root = fixture({ identifier: "com.example.pipeline" });
  assert.throws(
    () => validateReleaseIdentity({ rootDir: root }),
    /identifier must remain "com\.pipeline\.report".*release continuity/,
  );
});

test("requires practical minimum window dimensions", () => {
  for (const overrides of [{ minWidth: 900 }, { minHeight: 600 }, { minWidth: "1024" }]) {
    const root = fixture(overrides);
    assert.throws(
      () => validateReleaseIdentity({ rootDir: root }),
      /minimum dimensions of at least 1024x700/,
    );
  }
});

test("requires the declared macOS minimum to match the Poppler closure", () => {
  const root = fixture({ minimumSystemVersion: "14.0" });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /macOS minimum.*Poppler closure target/);
});

test("blocks Windows installer downgrades", () => {
  const root = fixture({ allowDowngrades: true });
  assert.throws(
    () => validateReleaseIdentity({ rootDir: root }),
    /windows\.allowDowngrades to false/,
  );
});

test("requires the self-contained Windows WebView2 installer", () => {
  const root = fixture({
    webviewInstallMode: { type: "downloadBootstrapper", silent: true },
  });
  assert.throws(
    () => validateReleaseIdentity({ rootDir: root }),
    /WebView2 offlineInstaller/,
  );
});

test("requires both generated release resource trees", () => {
  const root = fixture({ resources: ["resources/poppler/**/*"] });
  assert.throws(() => validateReleaseIdentity({ rootDir: root }), /resources\/notices/);
});
