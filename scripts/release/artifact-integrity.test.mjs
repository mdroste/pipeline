import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  verifyArtifactSbom,
  writeArtifactChecksum,
  writeArtifactSbom,
} from "./artifact-integrity.mjs";

test("writes a deterministic sha256sum record using the public asset name", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-artifact-integrity-"));
  const artifact = path.join(root, "internal-build-name.bin");
  const output = path.join(root, "Pipeline-2.3.4-Linux.AppImage.sha256");
  fs.writeFileSync(artifact, "pipeline release fixture\n");

  const result = await writeArtifactChecksum({
    artifact,
    assetName: "Pipeline-2.3.4-Linux.AppImage",
    output,
  });

  assert.equal(result.digest, "82b2cb3536cd580ad36552a4d3928cb4ecbf9336ed01382e237ac0c75b1ad2ec");
  assert.equal(
    fs.readFileSync(output, "utf8"),
    `${result.digest}  Pipeline-2.3.4-Linux.AppImage\n`,
  );
});

test("rejects unsafe public asset names", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-artifact-integrity-"));
  const artifact = path.join(root, "artifact");
  fs.writeFileSync(artifact, "x");
  await assert.rejects(
    writeArtifactChecksum({
      artifact,
      assetName: "../Pipeline.exe",
      output: path.join(root, "checksum"),
    }),
    /invalid release asset name/,
  );
});

test("rejects empty artifacts", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-artifact-integrity-"));
  const artifact = path.join(root, "artifact");
  fs.writeFileSync(artifact, "");
  await assert.rejects(
    writeArtifactChecksum({
      artifact,
      assetName: "Pipeline.exe",
      output: path.join(root, "checksum"),
    }),
    /missing or empty/,
  );
});

test("artifact SBOM binds the installer, build-input SBOM, and native provenance", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-artifact-sbom-"));
  const artifact = path.join(root, "internal.AppImage");
  const buildInputSbom = path.join(root, "build-input.cdx.json");
  const provenance = path.join(root, "provenance.json");
  const output = path.join(root, "artifact.cdx.json");
  fs.writeFileSync(artifact, "installer bytes\n");
  fs.writeFileSync(buildInputSbom, JSON.stringify({
    bomFormat: "CycloneDX",
    metadata: { component: { name: "Pipeline", version: "2.3.4" } },
  }));
  fs.writeFileSync(provenance, JSON.stringify({ platform: "linux" }));

  await writeArtifactSbom({
    artifact,
    assetName: "Pipeline-2.3.4-Linux.AppImage",
    version: "2.3.4",
    buildInputSbom,
    buildInputSbomAssetName: "Pipeline-2.3.4-Linux-Build-input-SBOM.cdx.json",
    provenance,
    provenanceAssetName: "Pipeline-2.3.4-Linux-Poppler-provenance.json",
    output,
  });

  const result = await verifyArtifactSbom({
    artifact,
    assetName: "Pipeline-2.3.4-Linux.AppImage",
    version: "2.3.4",
    buildInputSbom,
    buildInputSbomAssetName: "Pipeline-2.3.4-Linux-Build-input-SBOM.cdx.json",
    provenance,
    provenanceAssetName: "Pipeline-2.3.4-Linux-Poppler-provenance.json",
    artifactSbom: output,
  });
  assert.match(result.artifactDigest, /^[0-9a-f]{64}$/);
  const document = JSON.parse(fs.readFileSync(output, "utf8"));
  assert.equal(document.metadata.component.name, "Pipeline-2.3.4-Linux.AppImage");
  assert.equal(document.components[0].version, "2.3.4");
});

test("artifact SBOM verification rejects changed release evidence", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-artifact-sbom-"));
  const artifact = path.join(root, "Pipeline.exe");
  const buildInputSbom = path.join(root, "build-input.cdx.json");
  const provenance = path.join(root, "provenance.json");
  const output = path.join(root, "artifact.cdx.json");
  fs.writeFileSync(artifact, "installer bytes\n");
  fs.writeFileSync(buildInputSbom, JSON.stringify({
    bomFormat: "CycloneDX",
    metadata: { component: { name: "Pipeline", version: "2.3.4" } },
  }));
  fs.writeFileSync(provenance, JSON.stringify({ platform: "windows" }));
  const options = {
    artifact,
    assetName: "Pipeline-2.3.4-Windows.exe",
    version: "2.3.4",
    buildInputSbom,
    buildInputSbomAssetName: "Pipeline-2.3.4-Windows-Build-input-SBOM.cdx.json",
    provenance,
    provenanceAssetName: "Pipeline-2.3.4-Windows-Poppler-provenance.json",
  };
  await writeArtifactSbom({ ...options, output });
  fs.appendFileSync(buildInputSbom, "\n");
  await assert.rejects(
    verifyArtifactSbom({ ...options, artifactSbom: output }),
    /build-input-sbom-sha256/,
  );
});
