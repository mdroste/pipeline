import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createHash } from "node:crypto";
import test from "node:test";
import { prepareGpuRuntime, verifyGpuRuntime, gpuRuntimeAssets } from "./gpu-runtime.mjs";

test("runtime staging retains all backend files and binds exact installed bytes", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-gpu-test-"));
  try {
    const source = path.join(root, "source"), stage = path.join(root, "stage");
    fs.mkdirSync(source);
    for (const name of ["llama-server.exe", "llama-server-impl.dll", "ggml-vulkan.dll", "ggml-cpu-x64.dll"])
      fs.writeFileSync(path.join(source, name), name);
    prepareGpuRuntime(source, stage);
    const inventory = JSON.parse(fs.readFileSync(path.join(stage, "integrity.json")));
    assert.deepEqual(inventory.scopes, ["runtime"]);
    assert.equal(inventory.entries.length, 4);
    for (const entry of inventory.entries) {
      const bytes = fs.readFileSync(path.join(stage, entry.path));
      assert.equal(entry.size, bytes.length);
      assert.equal(entry.sha256, createHash("sha256").update(bytes).digest("hex"));
      assert.equal(entry.kind, "file");
      assert.equal(entry.permissions, 0);
    }
    assert.throws(() => prepareGpuRuntime(source, stage), /fresh directory/);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("the release verifier and Rust fixtures accept the same signed manifest", () => {
  const fixture = JSON.parse(fs.readFileSync(new URL("../../gui/src-tauri/src/engines/fixtures/gpu-runtime.json", import.meta.url)));
  const { manifest, signature } = fixture.windows;
  verifyGpuRuntime(Buffer.from("zip"), Buffer.from(manifest), signature, "1.5.3", fixture.public_key);
  assert.throws(() => verifyGpuRuntime(Buffer.from("tampered"), Buffer.from(manifest), signature, "1.5.3", fixture.public_key), /signed release manifest/);
  assert.throws(() => verifyGpuRuntime(Buffer.from("zip"), Buffer.from(manifest), signature, "9.9.9", fixture.public_key), /version/);
  assert.throws(() => verifyGpuRuntime(Buffer.from("zip"), Buffer.from("{}"), signature, "1.5.3", fixture.public_key), /signature/);
  assert.equal(gpuRuntimeAssets("1.5.3").length, 3);
});
