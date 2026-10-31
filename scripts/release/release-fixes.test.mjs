import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
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
    execFileSync(process.execPath, [path.join(root, "scripts/release/prepare-package-notices.mjs")], {
      env: { ...process.env, PIPELINE_OFFICIAL_RELEASE: "0" },
    });
    const output = path.join(root, "gui/src-tauri/resources/notices");
    const notices = fs.readFileSync(path.join(output, "THIRD_PARTY_LICENSES.md"), "utf8");
    assert.match(notices, /\[MIT License\]\(PIPELINE_LICENSE\.txt\)/);
    assert.equal(
      fs.readFileSync(path.join(output, "PIPELINE_LICENSE.txt"), "utf8"),
      fs.readFileSync(path.join(root, "LICENSE"), "utf8"),
    );
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
