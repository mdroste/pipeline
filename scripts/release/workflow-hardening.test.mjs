import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

// Windows runners check out with core.autocrlf=true, so every assertion below
// that spans a line boundary has to see LF regardless of the working tree.
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

test("CI and release use the repository's exact supported toolchains", () => {
  const nodeVersion = readText("gui", ".nvmrc").trim();
  const rustToolchain = readText("rust-toolchain.toml");
  const rustVersion = rustToolchain.match(/^channel\s*=\s*"([^"]+)"$/m)?.[1];
  assert.match(nodeVersion, /^\d+\.\d+\.\d+$/);
  assert.match(rustVersion, /^\d+\.\d+\.\d+$/);

  for (const workflow of ["build.yml", "release.yml"]) {
    const contents = readText(".github", "workflows", workflow);
    assert.ok(contents.includes(`NODE_VERSION: ${nodeVersion}`), `${workflow} Node version drifted`);
    assert.ok(contents.includes(`RUST_VERSION: ${rustVersion}`), `${workflow} Rust version drifted`);
    assert.ok(!contents.includes("toolchain: stable"), `${workflow} must not float Rust stable`);
    assert.ok(!/node-version:\s+(?:22|24)\s*$/m.test(contents), `${workflow} must not float a Node major`);
    assert.ok(
      contents.includes("cargo +1.88.0 check --locked"),
      `${workflow} must bypass the pinned default when checking the declared MSRV`,
    );
  }
  assert.ok(
    readText(".github", "workflows", "release.yml")
      .includes("cargo clippy --locked --all-targets --all-features -- -D warnings"),
    "manual release preflight must retain the locked all-feature Clippy gate",
  );

  const packageJson = JSON.parse(readText("gui", "package.json"));
  assert.equal(packageJson.packageManager, "npm@11.16.0");
  assert.match(packageJson.engines.node, new RegExp(`(?:\\^|>=)${nodeVersion.replaceAll(".", "\\.")}`));
});

test("build CI is manual-only and uses one Ubuntu runner at a time", () => {
  const contents = readText(".github", "workflows", "build.yml");

  for (const required of [
    "workflow_dispatch:",
    "profile:",
    "- quick",
    "- maintenance",
    "if: inputs.profile == 'quick'",
    "if: inputs.profile == 'maintenance'",
    "name: Linux unit checks",
    "name: Manual audits and MSRV",
    "cargo test --locked --all-targets",
    "npm test -- --run",
    "cargo +1.88.0 check --locked",
  ]) {
    assert.ok(contents.includes(required), `build workflow is missing ${required}`);
  }

  assert.equal([...contents.matchAll(/runs-on:\s+ubuntu-22\.04/g)].length, 2);
  assert.doesNotMatch(contents, /^\s+(?:push|pull_request|schedule):\s*$/m);
  assert.doesNotMatch(contents, /(?:windows|macos)-\d+/i);
  assert.doesNotMatch(contents, /npm run tauri|test:e2e/);
});

test("real parser qualification is manual and Linux-only", () => {
  const contents = readText(".github", "workflows", "paddle-parser-qualification.yml");
  assert.ok(contents.includes("workflow_dispatch:"));
  assert.ok(contents.includes("workflow_call:"));
  assert.ok(contents.includes("runs-on: ubuntu-22.04"));
  assert.ok(contents.includes("Real parser (Linux x64)"));
  assert.doesNotMatch(contents, /^\s*schedule:\s*$/m);
  assert.doesNotMatch(contents, /matrix:|windows-|macos-|ubuntu-24\.04-arm/);
});

test("Dependabot batches each ecosystem monthly without automatic CI", () => {
  const contents = readText(".github", "dependabot.yml");
  assert.equal([...contents.matchAll(/interval:\s+monthly/g)].length, 3);
  assert.equal([...contents.matchAll(/^\s+groups:\s*$/gm)].length, 3);
  assert.ok(contents.includes("CI is manual-only"));
});

test("release workflow retains signed tags, macOS signing, and completeness gates", () => {
  const contents = readText(".github", "workflows", "release.yml");
  for (const required of [
    "Require a verified signed annotated tag",
    "git/ref/tags/$encoded_tag",
    "git/tags/$tag_object_sha",
    ".object.type",
    ".verification.verified",
    ".verification.reason",
    'target_type="$(jq -r \'.object.type // empty\' <<<"$tag_json")"',
    'target_sha="$(jq -r \'.object.sha // empty\' <<<"$tag_json")"',
    'if [ "$target_type" != "commit" ]',
    'if [ "$target_sha" != "$GITHUB_SHA" ]',
    "is lightweight",
    "name: release",
    "Import Apple code-signing cert (macOS)",
    "APPLE_CERTIFICATE",
    "APPLE_SIGNING_IDENTITY",
    "APPLE_ID",
    "plutil -extract LSRequiresCarbon raw",
    "Windows packages are intentionally unsigned",
    "operation:",
    "- preflight",
    "- package",
    "- publish",
    "platform:",
    "if: inputs.operation == 'preflight'",
    "if: inputs.operation == 'package'",
    "if: inputs.operation == 'publish'",
    "Build package",
    "Publish validated platform assets to the draft",
    "gh release create \"${TAG}\" --draft --verify-tag",
    "gh release edit \"${TAG}\" --draft=false --latest",
    "actions/attest-build-provenance@",
    "anchore/scan-action@",
    "Artifact-SBOM.cdx.json",
    "Build-input-SBOM.cdx.json",
    "--artifact-sbom",
    "pipeline-release-evidence/*",
    "GRYPE_VERSION: v0.110.0",
    "SYFT_VERSION: 1.42.3",
    "SYFT_LINUX_AMD64_SHA256: 0d6be741479eddd2c8644a288990c04f3df0d609bbc1599a005532a9dff63509",
    "macOS-*) package_type=homebrew",
    "Windows) package_type=conda",
    "Linux) package_type=deb",
    'select(any(.properties[]?;',
    '.name == "pipeline:system-package" and .value == "true"',
    "select(.type == \"conda\")",
    "--conda-package-root",
    "aptSnapshot",
    "snapshot=${snapshot}",
    "libfuse2",
    "release-complete:",
    "validate-release-assets.mjs",
    "Full parser release qualification",
    "python scripts/release/qualify-paddle-parser.py",
    'git -C "$core_repo" checkout --force --detach "$core_commit"',
    "provenance_args=(",
    'provenance_args+=("${provenance_windows_args[@]}")',
    'node scripts/release/poppler-provenance.mjs "${provenance_args[@]}"',
  ]) {
    assert.ok(contents.includes(required), `release workflow is missing ${required}`);
  }

  const tagGate = contents.indexOf("Require a verified signed annotated tag");
  const firstCheckout = contents.indexOf("uses: actions/checkout@");
  assert.ok(tagGate >= 0 && tagGate < firstCheckout, "signed-tag verification must run before checkout");

  const checkoutCount = [...contents.matchAll(/uses:\s+actions\/checkout@/g)].length;
  const immutableCheckoutCount = [...contents.matchAll(/ref:\s+\$\{\{\s*github\.sha\s*\}\}/g)].length;
  assert.equal(
    immutableCheckoutCount,
    checkoutCount,
    "every release checkout must explicitly use the immutable workflow SHA",
  );
  assert.doesNotMatch(
    contents,
    /^\s*(?:push|pull_request|schedule):\s*$/m,
    "no repository event or schedule may automatically start the release workflow",
  );
  assert.doesNotMatch(
    contents,
    /name: Platform (?:Rust|frontend) tests/,
    "platform packaging must not repeat platform-neutral test suites",
  );
  assert.match(contents, /group: release-\$\{\{ github\.ref \}\}/);
  assert.match(contents, /cancel-in-progress: false/);
  assert.equal(
    [...contents.matchAll(/runs-on: ubuntu-22\.04/g)].length,
    2,
    "preflight and publish should each use one Linux runner",
  );
  const uploadStart = contents.indexOf('gh release upload "${TAG}"');
  const uploadEnd = contents.indexOf("--clobber", uploadStart);
  assert.ok(uploadStart >= 0 && uploadEnd > uploadStart);
  assert.doesNotMatch(
    contents.slice(uploadStart, uploadEnd),
    /#/,
    "release files must be staged under their exact public basenames, not labels",
  );
  assert.ok(
    contents.indexOf("Verify macOS artifact architecture")
      < contents.indexOf("Publish validated platform assets to the draft"),
    "platform validation must precede draft upload",
  );

  assert.equal(
    [...contents.matchAll(/bash scripts\/release\/validate-signed-tag-binding\.sh/g)].length,
    3,
    "publishing jobs must revalidate tag binding before build, upload, and final validation",
  );
  assert.equal(
    [...contents.matchAll(/plutil -extract LSRequiresCarbon raw/g)].length,
    2,
    "both macOS app and DMG validation must read the Carbon Boolean independent of plutil display formatting",
  );
  assert.ok(
    !contents.includes('grep -q \'"LSRequiresCarbon" => false\''),
    "macOS validation must not depend on plutil -p Boolean rendering",
  );
  assert.ok(
    !contents.includes("APPIMAGE_EXTRACT_AND_RUN"),
    "release smoke must execute the public AppImage entry point through FUSE",
  );
  assert.ok(
    contents.includes("/assets?per_page=100"),
    "draft asset lookup must paginate beyond GitHub's default page",
  );
  assert.ok(
    !contents.includes("WINDOWS_CERTIFICATE") &&
      !contents.includes("Get-AuthenticodeSignature") &&
      !contents.includes("tauri.windows-signing.conf.json"),
    "unsigned Windows releases must not require or claim Authenticode signing",
  );
  assert.ok(
    !contents.includes("provenance_extra=()"),
    "macOS Bash 3.2 must not expand an empty array under set -u",
  );
});

test("Linux release packages resolve only through the fixed Ubuntu snapshot", () => {
  const contents = readText(".github", "workflows", "release.yml");
  const releaseJob = contents.slice(
    contents.indexOf("\n  release:"),
    contents.indexOf("\n  release-complete:"),
  );
  assert.ok(releaseJob.includes('dpkg --compare-versions "${apt_version}" ge 2.4.11'));
  assert.equal(
    [...releaseJob.matchAll(/\[snapshot=\$\{snapshot\}/g)].length,
    3,
    "Jammy, updates, and security repositories must all use the fixed snapshot",
  );
  const aptCommands = [...releaseJob.matchAll(/^\s*sudo apt-get .+$/gm)].map((match) => match[0]);
  assert.ok(aptCommands.length >= 3, "release job should update/install through apt");
  for (const command of aptCommands) {
    assert.ok(
      command.includes('"${apt_options[@]}"'),
      `mutable apt command escaped snapshot options: ${command.trim()}`,
    );
  }
  assert.doesNotMatch(
    releaseJob,
    /^\s*sudo apt-get (?:update|install)\b/gm,
    "release apt commands must not bypass the fixed snapshot options",
  );
  const linuxInstallStep = releaseJob.slice(
    releaseJob.indexOf("- name: Install Linux dependencies"),
    releaseJob.indexOf("- name: Install npm dependencies"),
  );
  assert.equal(
    [...linuxInstallStep.matchAll(/^\s+patchelf(?:\s+\\)?$/gm)].length,
    1,
    "release Linux dependency list must not contain duplicate patchelf entries",
  );
});

test("Windows release uses a complete versioned conda attribution lock", () => {
  const lock = JSON.parse(
    readText("scripts", "release", "poppler-lock.json"),
  );
  const packages = lock.windows.packages;
  assert.equal(packages.length, 20);
  assert.equal(new Set(packages.map((item) => item.name)).size, packages.length);
  const files = packages.flatMap((item) => {
    for (const field of ["version", "build", "license", "sourceUrl"]) {
      assert.ok(item[field], `${item.name} is missing ${field}`);
    }
    assert.match(item.archiveSha256, /^[a-f0-9]{64}$/);
    assert.match(
      item.sourceUrl,
      new RegExp(
        `^https://conda\\.anaconda\\.org/conda-forge/win-64/${
          item.name.replaceAll("-", "\\-")
        }-${item.version.replaceAll(".", "\\.")}-${item.build}\\.conda$`,
      ),
    );
    assert.ok(item.files.length > 0, `${item.name} owns no shipped files`);
    return item.files;
  });
  assert.equal(files.length, 26);
  assert.equal(new Set(files.map((file) => file.path)).size, files.length);
  assert.ok(files.every((file) => /^[a-f0-9]{64}$/.test(file.sha256)));

  const workflow = readText(".github", "workflows", "release.yml");
  assert.ok(workflow.includes("packages: .windows.packages"));
  assert.ok(!workflow.includes(".windows.components"));
  assert.ok(workflow.includes("7z x -y \"$package_archive\""));
  assert.ok(workflow.includes("sha256sum --check --strict"));
  assert.ok(
    workflow.includes("tr -d '\\r'"),
    "Windows lock TSV must normalize jq CRLF before strict field validation",
  );
});

test("native SBOM scan uses the reviewed Anchore action and supported v7.4.0 inputs", () => {
  const contents = readText(".github", "workflows", "release.yml");
  const expectedUse = "anchore/scan-action@e1165082ffb1fe366ebaf02d8526e7c4989ea9d2 # v7.4.0";
  const scanSteps = contents.split(`uses: ${expectedUse}`).slice(1);
  assert.equal(scanSteps.length, 4);
  const supportedInputs = new Set([
    "sbom",
    "fail-build",
    "severity-cutoff",
    "output-format",
    "add-cpes-if-none",
    "grype-version",
    "cache-db",
  ]);
  for (const suffix of scanSteps) {
    const withBlock = suffix.match(/^\s*\n\s*with:\n((?:\s{10}.+\n)+)/)?.[1];
    assert.ok(withBlock, "Anchore scan step is missing its input block");
    const keys = [...withBlock.matchAll(/^\s{10}([a-z-]+):/gm)].map((match) => match[1]);
    assert.ok(keys.length > 0);
    for (const key of keys) {
      assert.ok(supportedInputs.has(key), `Anchore v7.4.0 does not declare input ${key}`);
    }
    assert.match(withBlock, /grype-version: \$\{\{ env\.GRYPE_VERSION \}\}/);
    assert.match(withBlock, /add-cpes-if-none: true/);
  }
});

test("reusable signed-tag check binds the current annotated tag to the event SHA", () => {
  const contents = readText("scripts", "release", "validate-signed-tag-binding.sh");
  for (const required of [
    "git/ref/tags/$encoded_tag",
    "git/tags/$tag_object_sha",
    '.verification.verified // false',
    'if [ "$object_type" != "tag" ]',
    'if [ "$target_type" != "commit" ]',
    'if [ "$target_sha" != "$GITHUB_SHA" ]',
  ]) {
    assert.ok(contents.includes(required), `signed-tag binding check is missing ${required}`);
  }
});
