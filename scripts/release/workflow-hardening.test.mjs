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
    "release-cache.yml",
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
  for (const workflow of ["build.yml", "release.yml", "release-cache.yml"]) {
    const contents = readText(".github", "workflows", workflow);
    assert.ok(contents.includes(`NODE_VERSION: ${nodeVersion}`));
    assert.ok(contents.includes(`RUST_VERSION: ${rustVersion}`));
    assert.ok(!contents.includes("toolchain: stable"));
  }
});

test("automatic CI gates production builds and all native targets", () => {
  const contents = readText(".github", "workflows", "build.yml");
  for (const required of ["push:", "pull_request:", "workflow_call:", "npm run build", "npm run test:release",
    "cargo clippy --locked --all-targets --all-features -- -D warnings", "cargo test --locked --all-targets",
    "cargo test --locked --lib engines::download -- --ignored", "macos-15", "windows-2022"])
    assert.ok(contents.includes(required), required);
});

test("clean checkouts retain a Poppler resource for native test builds", () => {
  const directory = path.join(ROOT, "gui", "src-tauri", "resources", "poppler");
  assert.ok(
    [".gitkeep", "pdftotext", "pdftotext.exe"].some((name) =>
      fs.existsSync(path.join(directory, name))),
    "Tauri's Poppler resource glob must match a file before native tests can compile",
  );
});

test("Windows PDF tests use the pinned Poppler tools before system executables", () => {
  const contents = readText(".github", "workflows", "build.yml");
  const setup = contents.split("- name: Install pinned Poppler for Windows PDF tests")[1]?.split("- name:")[0];
  assert.ok(setup, "Windows CI must install Poppler instead of using the runner's Xpdf");
  assert.match(setup, /if: runner\.os == 'Windows'/);
  assert.match(setup, /bash scripts\/release\/bundle-poppler\.sh windows/);
  assert.match(setup, /cygpath -w "\$PWD\/gui\/src-tauri\/resources\/poppler" >> "\$GITHUB_PATH"/);
});

test("release has one draft coordinator behind quality gates and no publication or overwrite", () => {
  const contents = readText(".github", "workflows", "release.yml");
  for (const required of ["workflow_dispatch:", "needs: prepare", "needs: [quality, package]", "fetch-depth: 0",
    "draft-release.mjs prepare", "draft-release.mjs upload", "draft-release.mjs verify", "bundle-poppler.sh",
    "Build unsigned package", "Import Apple signing certificate"])
    assert.ok(contents.includes(required), required);
  assert.doesNotMatch(contents, /--clobber|--draft=false|gh release edit/);
});

test("new release tags reuse dependencies compiled on main under the same cache key", () => {
  const release = readText(".github", "workflows", "release.yml");
  const warm = readText(".github", "workflows", "release-cache.yml");
  const cacheStep = (contents) => contents.split("name: Restore Rust build cache")[1]?.split("- name:")[0] ?? "";

  assert.match(cacheStep(release), /save-if: false/);
  assert.doesNotMatch(cacheStep(warm), /save-if/);
  for (const contents of [release, warm]) {
    for (const shared of ["CARGO_INCREMENTAL: 1", "workspaces: gui/src-tauri -> target",
      "shared-key: release-${{ matrix.id }}"])
      assert.ok(contents.includes(shared), shared);
    for (const runner of ["macos-15", "macos-15-intel", "windows-2022", "ubuntu-22.04"])
      assert.ok(contents.includes(runner), runner);
    for (const target of ["aarch64-apple-darwin", "x86_64-apple-darwin"])
      assert.ok(contents.includes(`--target ${target}`), target);
  }
  assert.ok(release.includes('MACOSX_DEPLOYMENT_TARGET: "15.0"'));
  assert.ok(warm.includes("&& '15.0' ||"), "dependencies must target the release's macOS version");
  assert.ok(warm.includes("npm run tauri -- build ${{ matrix.tauri_args }} --no-bundle -- --locked"));
  assert.ok(warm.includes("if: github.ref == 'refs/heads/main'"));
  assert.ok(warm.includes("contents: read"));
  assert.doesNotMatch(warm, /secrets\.|^\s*environment:|contents: write|id-token|actions\/cache@|bundle-poppler|pull_request/m);
});

test("every release build signs the in-app update package", () => {
  const release = readText(".github", "workflows", "release.yml");
  const matrix = [...release.matchAll(/"id":"(macos-[a-z0-9]+)"[^}]*"bundle":"([^"]+)"/g)];
  assert.ok(matrix.length >= 2);
  // Without the app target, Tauri builds no macOS update archive.
  for (const [, , bundle] of matrix) assert.equal(bundle, "app,dmg");
  for (const step of ["Build signed and notarized macOS package", "Build unsigned package", "Build signed Windows package"]) {
    const build = release.split(`name: ${step}`)[1]?.split("- name:")[0] ?? "";
    assert.ok(build.includes("TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}"), step);
    assert.ok(build.includes("TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}"), step);
  }
  for (const step of ["Build signed and notarized macOS package", "Build unsigned package"]) {
    const build = release.split(`name: ${step}`)[1]?.split("- name:")[0] ?? "";
    assert.ok(build.includes(`--config '{"bundle":{"createUpdaterArtifacts":true}}'`), step);
  }
  assert.ok(release.indexOf("draft-release.mjs manifest") < release.indexOf('draft-release.mjs verify'));
});

test("the Windows window differs from the others only by its native title bar", () => {
  const config = JSON.parse(readText("gui", "src-tauri", "tauri.conf.json"));
  const windows = JSON.parse(
    readText("gui", "src-tauri", "tauri.windows.conf.json"),
  );
  // Tauri replaces arrays when it merges platform config, so these can drift.
  assert.deepEqual(windows.app.windows, [
    { ...config.app.windows[0], decorations: false },
  ]);
  assert.deepEqual(Object.keys(windows), ["app"]);
  assert.deepEqual(Object.keys(windows.app), ["windows"]);
});

test("the window controls drawn by the app are allowed to act", () => {
  const capability = JSON.parse(
    readText("gui", "src-tauri", "capabilities", "default.json"),
  );
  for (const permission of ["close", "minimize", "toggle-maximize"]) {
    assert.ok(capability.permissions.includes(`core:window:allow-${permission}`));
  }
});

test("Tauri packages only the GUI and avoids persistent incremental caches", () => {
  const cargo = readText("gui", "src-tauri", "Cargo.toml");
  const config = JSON.parse(readText("gui", "src-tauri", "tauri.conf.json"));
  const capability = JSON.parse(
    readText("gui", "src-tauri", "capabilities", "default.json"),
  );
  assert.match(cargo, /\[\[bin\]\]\nname = "pipeline-gui"/);
  for (const profile of ["dev", "release"]) {
    const body = cargo.split(`[profile.${profile}]`)[1]?.split("\n[")[0];
    assert.match(body ?? "", /incremental = false/);
  }
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

test("current public version references match the application metadata", () => {
  const { version } = JSON.parse(readText("gui", "package.json"));
  assert.ok(readText("README.md").includes(`version-${version}-blue`));
  assert.ok(readText(".github", "ISSUE_TEMPLATE", "bug_report.yml").includes(`placeholder: "${version}"`));
  assert.ok(readText("CHANGELOG.md").includes(`## ${version} —`));
});
