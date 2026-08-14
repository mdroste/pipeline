import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const readText = (...segments) =>
  fs.readFileSync(path.join(ROOT, ...segments), "utf8").replaceAll("\r\n", "\n");

test("all third-party GitHub Actions are pinned to immutable commit SHAs", () => {
  for (const workflow of [
    "build.yml",
    "release.yml",
    "paddle-parser-qualification.yml",
  ]) {
    const contents = readText(".github", "workflows", workflow);
    const actions = [...contents.matchAll(/\buses:\s+([^@\s]+)@([^\s#]+)/g)];
    assert.ok(actions.length > 0, `${workflow} must use at least one Action`);
    for (const [, action, reference] of actions) {
      assert.match(
        reference,
        /^[0-9a-f]{40}$/,
        `${workflow}: ${action}@${reference} is not an immutable commit SHA`,
      );
    }
  }
});

test("CI and release use the repository's exact toolchains", () => {
  const nodeVersion = readText("gui", ".nvmrc").trim();
  const rustVersion = readText("rust-toolchain.toml")
    .match(/^channel\s*=\s*"([^"]+)"$/m)?.[1];
  for (const workflow of ["build.yml", "release.yml"]) {
    const contents = readText(".github", "workflows", workflow);
    assert.ok(contents.includes(`NODE_VERSION: ${nodeVersion}`));
    assert.ok(contents.includes(`RUST_VERSION: ${rustVersion}`));
    assert.ok(!contents.includes("toolchain: stable"));
  }
});

test("tests are manual, Linux-only, and limited to library and frontend tests", () => {
  const contents = readText(".github", "workflows", "build.yml");
  assert.ok(contents.includes("workflow_dispatch:"));
  assert.equal([...contents.matchAll(/runs-on:\s+ubuntu-22\.04/g)].length, 1);
  assert.ok(contents.includes("cargo test --locked --lib"));
  assert.ok(contents.includes("npm test -- --run"));
  assert.ok(contents.includes("cache-workspace-crates: true"));
  assert.doesNotMatch(contents, /^\s+(?:push|pull_request|schedule):\s*$/m);
  assert.doesNotMatch(contents, /cargo (?:clippy|audit)|npm audit|MSRV|macos|windows/i);
  assert.ok(contents.split("\n").length < 80, "test workflow should remain small");
});

test("release builds only installers from an explicit tag and keeps retries platform-local", () => {
  const contents = readText(".github", "workflows", "release.yml");
  for (const required of [
    "workflow_dispatch:",
    "platform:",
    "default: macos-arm64",
    "if: github.ref_type == 'tag'",
    '"id":"macos-arm64"',
    '"id":"macos-x64"',
    '"id":"windows"',
    '"id":"linux"',
    "cache-workspace-crates: true",
    "cache-on-failure: true",
    "bundle-poppler.sh",
    "Import Apple signing certificate",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "APPLE_PASSWORD",
    "APPLE_TEAM_ID",
    "--bundles ${{ matrix.bundle }}",
    "Build unsigned package",
    "gh release upload",
    "--clobber",
    "gh release edit",
  ]) {
    assert.ok(contents.includes(required), `release workflow is missing ${required}`);
  }

  assert.doesNotMatch(contents, /^\s+(?:push|pull_request|schedule):\s*$/m);
  assert.doesNotMatch(
    contents,
    /cargo (?:test|clippy|audit)|npm (?:test|audit)|SBOM|provenance|attest|qualify|smoke|Gatekeeper/i,
  );
  assert.doesNotMatch(
    contents,
    /WINDOWS_CERTIFICATE|Get-AuthenticodeSignature|tauri\.windows-signing/i,
  );
  assert.ok(contents.split("\n").length < 230, "release workflow should remain small");
});

test("Tauri packages only the GUI and uses a cache-friendly release profile", () => {
  const cargo = readText("gui", "src-tauri", "Cargo.toml");
  const config = JSON.parse(readText("gui", "src-tauri", "tauri.conf.json"));
  const capability = JSON.parse(
    readText("gui", "src-tauri", "capabilities", "default.json"),
  );
  assert.match(cargo, /\[\[bin\]\]\nname = "pipeline-gui"/);
  assert.match(cargo, /\[profile\.release\][\s\S]*incremental = true/);
  assert.doesNotMatch(cargo, /lto\s*=\s*true|codegen-units\s*=\s*1/);
  assert.deepEqual(config.bundle.targets, ["app", "dmg", "appimage", "nsis"]);
  assert.equal(config.build.beforeBundleCommand, "node ../scripts/release/sign-macos-cli.mjs");
  assert.ok(
    capability.permissions.includes("core:window:allow-destroy"),
    "close-requested listener must be allowed to destroy the native window",
  );
  assert.deepEqual(config.bundle.resources.filter((item) => item.includes("notices/")), [
    "resources/notices/NOTICE.txt",
    "resources/notices/PIPELINE_LICENSE.txt",
    "resources/notices/THIRD_PARTY_LICENSES.md",
  ]);
});

test("real parser qualification and Dependabot remain manual and infrequent", () => {
  const parser = readText(".github", "workflows", "paddle-parser-qualification.yml");
  assert.ok(parser.includes("workflow_dispatch:"));
  assert.ok(parser.includes("runs-on: ubuntu-22.04"));
  assert.doesNotMatch(parser, /^\s*schedule:\s*$/m);

  const dependabot = readText(".github", "dependabot.yml");
  assert.equal([...dependabot.matchAll(/interval:\s+monthly/g)].length, 3);
});
