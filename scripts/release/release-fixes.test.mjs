import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { libraryNotice } from "./library-licenses.mjs";
import { cliSigningPlan } from "./sign-macos-cli.mjs";
import { inventoryFiles, verifyInventory } from "./poppler-inventory.mjs";
import { qualify, captureCandidate, requiredPlatforms, requiredScenarios } from "./qualify-release.mjs";
import { installerNames } from "./draft-release.mjs";
test("generated package notices link to the bundled Pipeline license", () => {
  const repository = fileURLToPath(new URL("../../", import.meta.url));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-package-notices-"));
  try {
    for (const relative of [
      "scripts/release/prepare-package-notices.mjs",
      "scripts/release/library-licenses.mjs",
      "scripts/release/poppler-inventory.mjs",
      "scripts/release/validate-release-identity.mjs",
      "scripts/release/poppler-lock.json",
      "gui/package.json",
      "gui/package-lock.json",
      "gui/src-tauri/Cargo.toml",
      "gui/src-tauri/Cargo.lock",
      "gui/src-tauri/tauri.conf.json",
      "LICENSE",
      "THIRD_PARTY_LICENSES.md",
    ]) {
      const destination = path.join(root, relative);
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.copyFileSync(path.join(repository, relative), destination);
    }
    const prepare = () => execFileSync(process.execPath, [path.join(root, "scripts/release/prepare-package-notices.mjs")], {
      env: { ...process.env, PIPELINE_OFFICIAL_RELEASE: "0" },
    });
    prepare();
    const output = path.join(root, "gui/src-tauri/resources/notices");
    const ages = () => fs.readdirSync(output).map((name) => [name, fs.statSync(path.join(output, name)).mtimeMs]);
    const written = ages();
    prepare();
    assert.deepEqual(ages(), written, "an unchanged rewrite would recompile the app");
    const notices = fs.readFileSync(path.join(output, "THIRD_PARTY_LICENSES.md"), "utf8");
    assert.match(notices, /\[MIT License\]\(PIPELINE_LICENSE\.txt\)/);
    assert.equal(
      fs.readFileSync(path.join(output, "PIPELINE_LICENSE.txt"), "utf8"),
      fs.readFileSync(path.join(root, "LICENSE"), "utf8"),
    );
    assert.ok(fs.statSync(path.join(output, "THIRD_PARTY_LIBRARIES.txt")).size > 0);
    assert.match(fs.readFileSync(path.join(output, "NOTICE.txt"), "utf8"), /THIRD_PARTY_LIBRARIES\.txt/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
test("unsigned local macOS builds need no release identity; official builds do", () => {
  assert.equal(cliSigningPlan({platform:"darwin",env:{},root:"/repo"}), null);
  assert.throws(() => cliSigningPlan({platform:"darwin",env:{PIPELINE_OFFICIAL_RELEASE:"1"},root:"/repo"}), /required/);
  for (const [env, expected] of [[{},"target/release/pipeline-cli"], [{CARGO_TARGET_DIR:"custom"},"custom/release/pipeline-cli"], [{TAURI_ENV_TARGET_TRIPLE:"aarch64-apple-darwin"},"target/aarch64-apple-darwin/release/pipeline-cli"], [{PIPELINE_CLI_BINARY:"candidate/pipeline-cli"},"candidate/pipeline-cli"]]) {
    const result = cliSigningPlan({platform:"darwin",env:{APPLE_SIGNING_IDENTITY:"test",...env},root:"/repo",exists:()=>true});
    assert.equal(result.binary,path.resolve("/repo",expected));
  }
});
test("cached Poppler inventory detects changed, missing and added files", () => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),"pipeline-inventory-"));
  try {
    fs.writeFileSync(path.join(root,"pdftotext"),"test");
    fs.writeFileSync(path.join(root,"BUNDLE_INVENTORY.json"),JSON.stringify({schemaVersion:1,files:inventoryFiles(root)}));
    verifyInventory(root);
    fs.writeFileSync(path.join(root,"pdftotext"),"changed"); assert.throws(()=>verifyInventory(root),/changed/);
    fs.rmSync(path.join(root,"pdftotext")); assert.throws(()=>verifyInventory(root),/changed/);
    fs.writeFileSync(path.join(root,"pdftotext"),"test"); fs.writeFileSync(path.join(root,"extra"),"extra"); assert.throws(()=>verifyInventory(root),/changed/);
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
test("an official build won't ship without its library list", () => {
  const repository = fileURLToPath(new URL("../../", import.meta.url));
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-official-notices-"));
  try {
    for (const relative of ["gui/package-lock.json"]) {
      fs.mkdirSync(path.dirname(path.join(root, relative)), { recursive: true });
      fs.copyFileSync(path.join(repository, relative), path.join(root, relative));
    }
    assert.throws(() => libraryNotice({ root, version: "1.2.3", official: true }), /cargo metadata failed/);
    assert.match(libraryNotice({ root, version: "1.2.3", official: false }), /development build/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
test("the runbook's qualification commands work with installers in their own folder", () => {
  const repository = fileURLToPath(new URL("../../", import.meta.url));
  const script = path.join(repository, "scripts/release/qualify-release.mjs");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-runbook-"));
  const git = (...args) => execFileSync("git", ["-c", "user.name=Test", "-c", "user.email=test@example.test", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", ...args], { cwd: root, encoding: "utf8" }).trim();
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { cwd: root, encoding: "utf8" });
  try {
    git("init", "-q");
    git("commit", "-q", "--allow-empty", "-m", "candidate");
    const version = "9.9.9", commit = git("rev-parse", "HEAD");
    git("tag", `v${version}`);
    const installers = path.join(root, "draft-installers");
    fs.mkdirSync(installers);
    for (const name of installerNames(version)) {
      fs.writeFileSync(path.join(installers, name), name);
      fs.writeFileSync(path.join(installers, `${name}.sha256`), `${createHash("sha256").update(name).digest("hex")}  ${name}\n`);
    }
    const captured = run("--candidate", commit, version, installers);
    assert.equal(captured.status, 0, captured.stderr);
    const candidate = JSON.parse(captured.stdout);
    const candidatePath = path.join(root, "candidate.json");
    fs.writeFileSync(candidatePath, captured.stdout);

    fs.writeFileSync(path.join(root, "notes.txt"), "observed");
    const row = { status: "passed", observed: "observed", expected: "expected", environment: "test", versions: "test",
      evidence: [{ path: "notes.txt", sha256: createHash("sha256").update("observed").digest("hex") }] };
    const evidence = path.join(root, "evidence.json");
    fs.writeFileSync(evidence, JSON.stringify({ schemaVersion: 1, version, commit,
      scenarios: Object.fromEntries(requiredScenarios.map((name) => [name, row])),
      platforms: Object.fromEntries(requiredPlatforms.map((name) => [name, { ...row,
        installer: candidate.installers[name].name, installerSha256: candidate.installers[name].sha256 }])) }));

    const passed = run(evidence, candidatePath, "--installers", installers);
    assert.equal(passed.status, 0, passed.stderr);
    assert.match(passed.stdout, /qualification passed/);

    const unknown = "a".repeat(40);
    assert.match(run("--candidate", unknown, version, installers).stderr, /points at/);
    assert.match(run("--candidate", commit, "8.8.8", installers).stderr, /isn't in this repository/);
    fs.writeFileSync(candidatePath, JSON.stringify({ ...candidate, commit: unknown }));
    assert.match(run(evidence, candidatePath, "--installers", installers).stderr, /points at/);
    assert.match(run(evidence, candidatePath).stderr, /Usage/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
test("qualification rejects pending, different commits and changed evidence", () => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),"pipeline-qualification-"));
  try {
    const hash=createHash("sha256").update("fixture").digest("hex"); fs.writeFileSync(path.join(root,"evidence.txt"),"fixture");
    const row={status:"passed",observed:"Synthetic validator fixture",expected:"fixture",environment:"test",versions:"fixture",evidence:[{path:"evidence.txt",sha256:hash}]};
    const version = "0.9.5", commit = "a".repeat(40);
    for (const name of installerNames(version)) {
      fs.writeFileSync(path.join(root, name), name);
      fs.writeFileSync(path.join(root, `${name}.sha256`), `${createHash("sha256").update(name).digest("hex")}  ${name}\n`);
    }
    const candidate = captureCandidate(commit, version, root);
    const record={schemaVersion:1,version,commit,scenarios:Object.fromEntries(requiredScenarios.map(s=>[s,{...row}])),platforms:Object.fromEntries(requiredPlatforms.map(p=>[p,{...row,installer:candidate.installers[p].name,installerSha256:candidate.installers[p].sha256}]))};
    assert.equal(qualify(record,candidate,root),true);
    assert.throws(()=>qualify(record,{...candidate,commit:"b".repeat(40)},root),/exact/);
    assert.throws(()=>qualify({...record,version:"0.9.4"},candidate,root),/version/);
    const wrong = structuredClone(record);
    wrong.platforms.windows = { ...wrong.platforms.linux };
    assert.throws(()=>qualify(wrong,candidate,root),/installer differs/);
    for (const p of requiredPlatforms) wrong.platforms[p] = {...row,installer:"evidence.txt",installerSha256:hash};
    assert.throws(()=>qualify(wrong,candidate,root),/installer differs/);
    const renamed = structuredClone(record);
    renamed.platforms.windows.installerSha256 = hash;
    fs.writeFileSync(path.join(root, renamed.platforms.windows.installer), "fixture");
    assert.throws(()=>qualify(renamed,candidate,root),/installer differs/);
    assert.throws(()=>qualify(record,candidate,root),/checksum/);
    const win = candidate.installers.windows.name;
    fs.writeFileSync(path.join(root,win),win);
    const incomplete = structuredClone(candidate); delete incomplete.installers.linux;
    assert.throws(()=>qualify(record,incomplete,root),/identity/);
    record.scenarios.automations.status="pending"; assert.throws(()=>qualify(record,candidate,root),/Missing/); record.scenarios.automations.status="passed";
    fs.writeFileSync(path.join(root,"evidence.txt"),"changed"); assert.throws(()=>qualify(record,candidate,root),/checksum/);
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
