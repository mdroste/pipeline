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
    "advisories.yml",
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
  for (const workflow of ["build.yml", "release.yml", "release-cache.yml", "advisories.yml"]) {
    const contents = readText(".github", "workflows", workflow);
    assert.ok(contents.includes(`NODE_VERSION: ${nodeVersion}`));
    assert.ok(contents.includes(`RUST_VERSION: ${rustVersion}`));
    assert.ok(!contents.includes("toolchain: stable"));
  }
});

test("package.json admits only the pinned Node major, where the frontend tests pass", () => {
  const pinned = readText("gui", ".nvmrc").trim();
  const { engines } = JSON.parse(readText("gui", "package.json"));
  const major = pinned.split(".")[0];
  assert.equal(engines.node, `>=${pinned} <${Number(major) + 1}`);
  const lock = JSON.parse(readText("gui", "package-lock.json"));
  assert.equal(lock.packages[""].engines.node, engines.node);
});

test("release checkouts leave no token in .git/config for install scripts to read", () => {
  const release = readText(".github", "workflows", "release.yml");
  const checkouts = release.split(/\n(?=\s*- (?:uses|name): )/).filter((step) => step.includes("uses: actions/checkout@"));
  assert.ok(checkouts.length >= 3);
  for (const step of checkouts) assert.match(step, /^\s+persist-credentials: false$/m);
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

test("a clean checkout prepares the bundled notices before Tauri compiles", () => {
  const { build, bundle } = JSON.parse(readText("gui", "src-tauri", "tauri.conf.json"));
  const prepare = /^npm run prepare:release-resources && /;
  assert.match(build.beforeDevCommand, prepare);
  assert.match(build.beforeBuildCommand, prepare);
  // Exact paths fail tauri-build when missing, so the prepare step writes each.
  const writers = readText("scripts", "release", "prepare-package-notices.mjs")
    + readText("scripts", "release", "library-licenses.mjs");
  for (const item of bundle.resources.filter((name) => name.startsWith("resources/notices/")))
    assert.ok(writers.includes(path.basename(item)), item);
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
    "resources/notices/THIRD_PARTY_LIBRARIES.txt",
    "resources/notices/THIRD_PARTY_LICENSES.md",
  ]);
});

test("packaged apps are checked and launched before any upload", () => {
  const release = readText(".github", "workflows", "release.yml");
  const upload = release.indexOf("name: Upload immutable installer to draft");
  const macos = release.split("name: Check the macOS app's binaries, Poppler, and launch")[1]?.split("- name:")[0] ?? "";
  const linux = release.split("name: Check Poppler and launch the AppImage")[1]?.split("- name:")[0] ?? "";
  for (const step of ["Check the macOS app's binaries, Poppler, and launch", "Check Poppler and launch the AppImage"])
    assert.ok(release.indexOf(`name: ${step}`) < upload, step);
  for (const required of ["validate-macos-machos.sh", "--poppler-layout", "--require-developer-id", "--max-minos 15.0",
    "smoke-poppler.mjs", "smoke-packaged-app.mjs"])
    assert.ok(macos.includes(required), required);
  assert.ok(linux.includes("smoke-poppler.mjs"));
  assert.ok(linux.includes("xvfb-run -a node scripts/release/smoke-packaged-app.mjs --process-stability-only"));
  assert.match(release, /libfuse2 patchelf poppler-utils xvfb/);
  assert.ok(readText("scripts", "release", "verify-windows-package.ps1").includes("smoke-poppler.mjs"));

  const config = JSON.parse(readText("gui", "src-tauri", "tauri.conf.json"));
  assert.equal(config.bundle.macOS.minimumSystemVersion, "15.0", "keep --max-minos in step");
});

test("the final job checks the asset list and the tag after verifying the draft", () => {
  const release = readText(".github", "workflows", "release.yml");
  const verify = release.split("verify-draft:")[1];
  const order = ["draft-release.mjs verify", "validate-release-assets.mjs --version", "validate-signed-tag-binding.sh"];
  const positions = order.map((item) => verify.indexOf(item));
  assert.ok(positions.every((position) => position >= 0), order.join());
  assert.deepEqual(positions, [...positions].sort((left, right) => left - right));
});

test("every release script runs somewhere", () => {
  const folder = path.join(ROOT, "scripts", "release");
  const scripts = fs.readdirSync(folder).filter((name) => /\.(mjs|sh|ps1|py)$/.test(name));
  const workflows = fs.readdirSync(path.join(ROOT, ".github", "workflows"))
    .map((name) => readText(".github", "workflows", name));
  const callers = [...workflows, readText("gui", "package.json"), readText("gui", "src-tauri", "tauri.conf.json")];
  const within = (name) => scripts.filter((other) => other !== name).map((other) => readText("scripts", "release", other));
  // Run by hand: the runbook gives the command, and art changes are rare.
  const manual = { "qualify-release.mjs": "docs/releasing.md", "render-installer-art.py": "CLAUDE.md" };
  for (const name of scripts) {
    if (name === "release.test.mjs") continue;
    if (manual[name]) {
      assert.ok(readText(manual[name]).includes(name), name);
      continue;
    }
    assert.ok([...callers, ...within(name)].some((text) => text.includes(name)), `${name} has no caller`);
  }
});

test("dependency advisories are checked on a schedule with a pinned tool", () => {
  const advisories = readText(".github", "workflows", "advisories.yml");
  for (const required of ["schedule:", "workflow_dispatch:", "cargo install cargo-audit --version ${{ env.CARGO_AUDIT_VERSION }} --locked",
    "cargo audit", "npm audit --omit=dev"])
    assert.ok(advisories.includes(required), required);
  assert.match(advisories, /CARGO_AUDIT_VERSION: \d+\.\d+\.\d+\n/);
  assert.doesNotMatch(advisories, /continue-on-error|pull_request|workflow_call/);
  assert.ok(readText("gui", "src-tauri", ".cargo", "audit.toml").includes("advisories.yml"));
  // A new advisory must not stop a release; the runbook has it reviewed instead.
  for (const workflow of ["build.yml", "release.yml"])
    assert.ok(!readText(".github", "workflows", workflow).includes("advisories.yml"));
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
