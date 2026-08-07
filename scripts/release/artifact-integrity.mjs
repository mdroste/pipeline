#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { pipeline } from "node:stream/promises";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const STABLE_VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function safeAssetName(assetName) {
  if (
    typeof assetName !== "string"
    || !assetName
    || assetName.includes("/")
    || assetName.includes("\\")
    || /[\r\n\0]/.test(assetName)
  ) {
    throw new Error(`invalid release asset name: ${JSON.stringify(assetName)}`);
  }
  return assetName;
}

function safeVersion(version) {
  if (typeof version !== "string" || !STABLE_VERSION.test(version)) {
    throw new Error(`invalid stable release version: ${JSON.stringify(version)}`);
  }
  return version;
}

function readJson(file, description) {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch (error) {
    throw new Error(`${description} is not valid JSON: ${error.message}`);
  }
}

function deterministicUuid(seed) {
  const digest = crypto.createHash("sha256").update(seed).digest("hex");
  return [
    digest.slice(0, 8),
    digest.slice(8, 12),
    `5${digest.slice(13, 16)}`,
    `a${digest.slice(17, 20)}`,
    digest.slice(20, 32),
  ].join("-");
}

function propertyMap(component) {
  return new Map((component?.properties ?? []).map((item) => [item.name, item.value]));
}

export async function sha256File(file) {
  const hash = crypto.createHash("sha256");
  await pipeline(fs.createReadStream(file), hash);
  return hash.digest("hex");
}

export async function writeArtifactChecksum({ artifact, assetName, output }) {
  const name = safeAssetName(assetName);
  const metadata = fs.statSync(artifact);
  if (!metadata.isFile() || metadata.size === 0) {
    throw new Error(`release artifact is missing or empty: ${artifact}`);
  }
  const digest = await sha256File(artifact);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, `${digest}  ${name}\n`, { mode: 0o644 });
  return { artifact: path.resolve(artifact), assetName: name, digest, output: path.resolve(output) };
}

export async function writeArtifactSbom({
  artifact,
  assetName,
  version,
  buildInputSbom,
  buildInputSbomAssetName,
  provenance,
  provenanceAssetName,
  output,
}) {
  const publicName = safeAssetName(assetName);
  const releaseVersion = safeVersion(version);
  const inputName = safeAssetName(buildInputSbomAssetName);
  const provenanceName = safeAssetName(provenanceAssetName);
  const buildInputs = readJson(buildInputSbom, "build-input SBOM");
  const popplerProvenance = readJson(provenance, "Poppler provenance");
  if (buildInputs.bomFormat !== "CycloneDX") {
    throw new Error("build-input SBOM must be a CycloneDX document");
  }
  if (buildInputs.metadata?.component?.version !== releaseVersion) {
    throw new Error("build-input SBOM version does not match the release");
  }
  if (!["macos", "linux", "windows"].includes(popplerProvenance.platform)) {
    throw new Error("Poppler provenance has an invalid platform");
  }

  const [artifactDigest, buildInputDigest, provenanceDigest] = await Promise.all([
    sha256File(artifact),
    sha256File(buildInputSbom),
    sha256File(provenance),
  ]);
  const artifactRef = `pipeline:release-asset:${publicName}`;
  const applicationRef = `pkg:github/mdroste/pipeline@${encodeURIComponent(releaseVersion)}`;
  const releaseBase = `https://github.com/mdroste/pipeline/releases/download/v${releaseVersion}`;
  const document = {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: `urn:uuid:${deterministicUuid(
      `${releaseVersion}\0${publicName}\0${artifactDigest}\0${buildInputDigest}\0${provenanceDigest}`,
    )}`,
    version: 1,
    metadata: {
      component: {
        type: "file",
        "bom-ref": artifactRef,
        name: publicName,
        version: releaseVersion,
        hashes: [{ alg: "SHA-256", content: artifactDigest }],
        externalReferences: [
          {
            type: "bom",
            url: `${releaseBase}/${encodeURIComponent(inputName)}`,
            hashes: [{ alg: "SHA-256", content: buildInputDigest }],
          },
          {
            type: "build-meta",
            url: `${releaseBase}/${encodeURIComponent(provenanceName)}`,
            hashes: [{ alg: "SHA-256", content: provenanceDigest }],
          },
        ],
        properties: [
          { name: "pipeline:scope", value: "packaged release artifact" },
          { name: "pipeline:public-asset-name", value: publicName },
          { name: "pipeline:build-input-sbom-asset", value: inputName },
          { name: "pipeline:build-input-sbom-sha256", value: buildInputDigest },
          { name: "pipeline:poppler-provenance-asset", value: provenanceName },
          { name: "pipeline:poppler-provenance-sha256", value: provenanceDigest },
          { name: "pipeline:poppler-platform", value: popplerProvenance.platform },
        ],
      },
      properties: [
        {
          name: "pipeline:artifact-sbom-policy",
          value: "installer hash bound to reviewed build-input SBOM and Poppler provenance",
        },
      ],
    },
    components: [{
      type: "application",
      "bom-ref": applicationRef,
      name: "Pipeline",
      version: releaseVersion,
      licenses: [{ license: { id: "MIT" } }],
    }],
    dependencies: [
      { ref: artifactRef, dependsOn: [applicationRef] },
      { ref: applicationRef, dependsOn: [] },
    ],
  };
  fs.mkdirSync(path.dirname(output), { recursive: true });
  fs.writeFileSync(output, `${JSON.stringify(document, null, 2)}\n`, { mode: 0o644 });
  return {
    artifact: path.resolve(artifact),
    artifactDigest,
    buildInputDigest,
    provenanceDigest,
    output: path.resolve(output),
  };
}

export async function verifyArtifactSbom({
  artifact,
  assetName,
  version,
  buildInputSbom,
  buildInputSbomAssetName,
  provenance,
  provenanceAssetName,
  artifactSbom,
}) {
  const publicName = safeAssetName(assetName);
  const releaseVersion = safeVersion(version);
  const inputName = safeAssetName(buildInputSbomAssetName);
  const provenanceName = safeAssetName(provenanceAssetName);
  const document = readJson(artifactSbom, "artifact SBOM");
  if (document.bomFormat !== "CycloneDX" || document.specVersion !== "1.5") {
    throw new Error("artifact SBOM must be a CycloneDX 1.5 document");
  }
  const component = document.metadata?.component;
  if (
    component?.type !== "file"
    || component["bom-ref"] !== `pipeline:release-asset:${publicName}`
    || component.name !== publicName
    || component.version !== releaseVersion
  ) {
    throw new Error("artifact SBOM identity does not match the release asset");
  }
  const [artifactDigest, buildInputDigest, provenanceDigest] = await Promise.all([
    sha256File(artifact),
    sha256File(buildInputSbom),
    sha256File(provenance),
  ]);
  const recordedArtifactDigest = component.hashes?.find((item) => item.alg === "SHA-256")?.content;
  if (recordedArtifactDigest !== artifactDigest) {
    throw new Error("artifact SBOM installer SHA-256 does not match the downloaded asset");
  }
  const properties = propertyMap(component);
  for (const [key, expected] of [
    ["pipeline:scope", "packaged release artifact"],
    ["pipeline:public-asset-name", publicName],
    ["pipeline:build-input-sbom-asset", inputName],
    ["pipeline:build-input-sbom-sha256", buildInputDigest],
    ["pipeline:poppler-provenance-asset", provenanceName],
    ["pipeline:poppler-provenance-sha256", provenanceDigest],
  ]) {
    if (properties.get(key) !== expected) {
      throw new Error(`artifact SBOM ${key} does not match the downloaded release evidence`);
    }
  }
  const releaseBase = `https://github.com/mdroste/pipeline/releases/download/v${releaseVersion}`;
  for (const [type, name, digest] of [
    ["bom", inputName, buildInputDigest],
    ["build-meta", provenanceName, provenanceDigest],
  ]) {
    const reference = component.externalReferences?.find((item) => item.type === type);
    const recordedDigest = reference?.hashes?.find((item) => item.alg === "SHA-256")?.content;
    if (
      reference?.url !== `${releaseBase}/${encodeURIComponent(name)}`
      || recordedDigest !== digest
    ) {
      throw new Error(`artifact SBOM ${type} reference does not match the downloaded release evidence`);
    }
  }
  return { artifactDigest, buildInputDigest, provenanceDigest };
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--verify") options.verify = true;
    else if (argument === "--artifact") options.artifact = argv[++index];
    else if (argument === "--asset-name") options.assetName = argv[++index];
    else if (argument === "--output") options.output = argv[++index];
    else if (argument === "--github-output") options.githubOutput = argv[++index];
    else if (argument === "--version") options.version = argv[++index];
    else if (argument === "--build-input-sbom") options.buildInputSbom = argv[++index];
    else if (argument === "--build-input-sbom-asset-name") options.buildInputSbomAssetName = argv[++index];
    else if (argument === "--provenance") options.provenance = argv[++index];
    else if (argument === "--provenance-asset-name") options.provenanceAssetName = argv[++index];
    else if (argument === "--artifact-sbom-output") options.artifactSbomOutput = argv[++index];
    else if (argument === "--artifact-sbom") options.artifactSbom = argv[++index];
    else throw new Error(`unknown argument: ${argument}`);
  }
  const requiredOptions = options.verify
    ? [
      "artifact",
      "assetName",
      "version",
      "buildInputSbom",
      "buildInputSbomAssetName",
      "provenance",
      "provenanceAssetName",
      "artifactSbom",
    ]
    : ["artifact", "assetName", "output"];
  for (const required of requiredOptions) {
    if (!options[required]) throw new Error(`--${required.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)} is required`);
  }
  if (!options.verify && options.artifactSbomOutput) {
    for (const required of [
      "version",
      "buildInputSbom",
      "buildInputSbomAssetName",
      "provenance",
      "provenanceAssetName",
    ]) {
      if (!options[required]) throw new Error(`--${required.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)} is required`);
    }
  }
  return options;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    const options = parseArgs(process.argv.slice(2));
    if (options.verify) {
      const result = await verifyArtifactSbom(options);
      console.log(`Verified artifact SBOM binding for ${options.assetName}: ${result.artifactDigest}`);
    } else {
      const result = await writeArtifactChecksum(options);
      if (options.artifactSbomOutput) {
        await writeArtifactSbom({
          ...options,
          output: options.artifactSbomOutput,
        });
      }
      if (options.githubOutput) {
        fs.appendFileSync(
          options.githubOutput,
          [
            `artifact_path=${result.artifact}`,
            `asset_name=${result.assetName}`,
            `checksum_path=${result.output}`,
            `sha256=${result.digest}`,
            "",
          ].join("\n"),
        );
      }
      console.log(`${result.digest}  ${result.assetName}`);
    }
  } catch (error) {
    console.error(`Release artifact checksum failed: ${error.message}`);
    process.exitCode = 1;
  }
}
