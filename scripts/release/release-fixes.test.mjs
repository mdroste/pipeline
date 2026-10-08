import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import { cliSigningPlan } from "./sign-macos-cli.mjs";
import { inventoryFiles, verifyInventory } from "./poppler-inventory.mjs";
import { qualify, requiredPlatforms, requiredScenarios } from "./qualify-release.mjs";
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
    const record={schemaVersion:1,version:"0.9.5",commit:"a".repeat(40),scenarios:Object.fromEntries(requiredScenarios.map(s=>[s,{...row}])),platforms:Object.fromEntries(requiredPlatforms.map(p=>[p,{...row,installer:"evidence.txt",installerSha256:hash}]))};
    assert.equal(qualify(record,record.commit,root),true);
    assert.throws(()=>qualify(record,"b".repeat(40),root),/exact/);
    record.scenarios.automations.status="pending"; assert.throws(()=>qualify(record,record.commit,root),/Missing/); record.scenarios.automations.status="passed";
    fs.writeFileSync(path.join(root,"evidence.txt"),"changed"); assert.throws(()=>qualify(record,record.commit,root),/checksum/);
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
