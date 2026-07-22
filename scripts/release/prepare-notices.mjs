#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import { validateReleaseIdentity } from "./validate-release-identity.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = path.resolve(path.dirname(SCRIPT_PATH), "../..");

function componentHash(alg, value) {
  return value ? [{ alg, content: value }] : undefined;
}

function cargoPurl(name, version) {
  return `pkg:cargo/${encodeURIComponent(name)}@${encodeURIComponent(version)}`;
}

function npmPurl(name, version) {
  const encoded = name.split("/").map(encodeURIComponent).join("/");
  return `pkg:npm/${encoded}@${encodeURIComponent(version)}`;
}

export function cargoComponents(contents) {
  const components = [];
  for (const block of contents.split(/(?=^\[\[package\]\]\s*$)/m)) {
    const name = block.match(/^name\s*=\s*"([^"]+)"\s*$/m)?.[1];
    const version = block.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
    const source = block.match(/^source\s*=\s*"([^"]+)"\s*$/m)?.[1];
    const checksum = block.match(/^checksum\s*=\s*"([a-fA-F0-9]{64})"\s*$/m)?.[1];
    if (!name || !version || !source) continue;
    const purl = cargoPurl(name, version);
    components.push({
      type: "library",
      "bom-ref": purl,
      group: "cargo",
      name,
      version,
      purl,
      hashes: componentHash("SHA-256", checksum),
      properties: [{ name: "pipeline:lock-source", value: source }],
    });
  }
  return components;
}

function npmName(lockPath, entry) {
  if (entry.name) return entry.name;
  const marker = "node_modules/";
  const index = lockPath.lastIndexOf(marker);
  return index >= 0 ? lockPath.slice(index + marker.length) : undefined;
}

export function npmComponents(lock) {
  const byReference = new Map();
  for (const [lockPath, entry] of Object.entries(lock.packages ?? {})) {
    if (!lockPath || !entry?.version) continue;
    const name = npmName(lockPath, entry);
    if (!name) continue;
    const purl = npmPurl(name, entry.version);
    if (byReference.has(purl)) continue;
    let hashes;
    const integrity = entry.integrity?.match(/^sha512-(.+)$/);
    if (integrity) hashes = componentHash("SHA-512", Buffer.from(integrity[1], "base64").toString("hex"));
    const component = {
      type: "library",
      "bom-ref": purl,
      group: "npm",
      name,
      version: entry.version,
      purl,
      hashes,
    };
    if (typeof entry.license === "string" && entry.license.trim()) {
      component.licenses = [{ license: { name: entry.license.trim() } }];
    }
    byReference.set(purl, component);
  }
  return [...byReference.values()];
}

function popplerComponents(provenance) {
  if (!provenance) return [];
  const components = [{
    type: "application",
    "bom-ref": `pkg:generic/poppler@${encodeURIComponent(provenance.version)}?platform=${provenance.platform}`,
    name: "poppler command-line utilities",
    version: provenance.version,
    licenses: [{ expression: provenance.license }],
    externalReferences: [{ type: "distribution", url: provenance.source.url }],
    properties: [
      { name: "pipeline:binary-provider", value: provenance.binaryProvider },
      { name: "pipeline:platform", value: provenance.platform },
    ],
  }];

  for (const item of provenance.packageInventory?.packages ?? []) {
    if (!item.name) continue;
    const reference = `pipeline:system-package:${provenance.platform}:${item.name}@${item.version ?? "unknown"}`;
    const component = {
      type: "library",
      "bom-ref": reference,
      group: provenance.packageInventory.manager ?? "system",
      name: item.name,
      properties: [{ name: "pipeline:system-package", value: "true" }],
    };
    if (item.version) component.version = item.version;
    if (typeof item.license === "string" && item.license) {
      component.licenses = [{ license: { name: item.license } }];
    }
    components.push(component);
  }

  for (const file of provenance.files ?? []) {
    components.push({
      type: "file",
      "bom-ref": `pipeline:bundled-file:poppler/${file.path}`,
      name: `poppler/${file.path}`,
      hashes: [{ alg: "SHA-256", content: file.sha256 }],
    });
  }
  return components;
}

export function buildSbom({ version, cargoLock, packageLock, provenance, cargoMetadata }) {
  const cargo = cargoComponents(cargoLock);
  const metadataByPackage = new Map(
    (cargoMetadata?.packages ?? []).map((item) => [`${item.name}@${item.version}`, item]),
  );
  for (const component of cargo) {
    const metadata = metadataByPackage.get(`${component.name}@${component.version}`);
    if (metadata?.license) component.licenses = [{ license: { name: metadata.license } }];
  }
  const components = [
    ...cargo,
    ...npmComponents(packageLock),
    ...popplerComponents(provenance),
  ].sort((a, b) => a["bom-ref"].localeCompare(b["bom-ref"]));

  const identity = crypto
    .createHash("sha256")
    .update(JSON.stringify({ version, refs: components.map((item) => item["bom-ref"]) }))
    .digest("hex");
  const uuid = `${identity.slice(0, 8)}-${identity.slice(8, 12)}-5${identity.slice(13, 16)}-a${identity.slice(17, 20)}-${identity.slice(20, 32)}`;
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: `urn:uuid:${uuid}`,
    version: 1,
    metadata: {
      component: {
        type: "application",
        "bom-ref": `pkg:github/mdroste/pipeline@${encodeURIComponent(version)}`,
        name: "Pipeline",
        version,
        licenses: [{ license: { id: "MIT" } }],
      },
      properties: [
        { name: "pipeline:scope", value: "locked Rust, npm, and platform Poppler release inputs" },
        { name: "pipeline:generator", value: "scripts/release/prepare-notices.mjs" },
      ],
    },
    components,
  };
}

function safeName(value) {
  return value.replace(/[^0-9A-Za-z._-]+/g, "_");
}

function copyLicenseFiles(sourceDir, destination, explicitFile) {
  const candidates = [];
  if (explicitFile && fs.existsSync(explicitFile)) candidates.push(explicitFile);
  if (fs.existsSync(sourceDir)) {
    for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
      if (entry.isFile() && /^(copying|copyright|licen[cs]e|notice)(?:[._-].*)?$/i.test(entry.name)) {
        candidates.push(path.join(sourceDir, entry.name));
      }
    }
  }
  const unique = [...new Set(candidates.map((file) => path.resolve(file)))];
  for (const file of unique) {
    if (fs.statSync(file).size > 2 * 1024 * 1024) continue;
    fs.mkdirSync(destination, { recursive: true });
    fs.copyFileSync(file, path.join(destination, path.basename(file)));
  }
}

function cargoMetadata(tauriDir, required) {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--manifest-path", path.join(tauriDir, "Cargo.toml")],
    { encoding: "utf8", maxBuffer: 100 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    if (required) throw new Error(`cargo metadata failed: ${(result.stderr || result.error?.message || "unknown error").trim()}`);
    console.warn("cargo metadata unavailable; development SBOM omits Cargo license metadata");
    return undefined;
  }
  return JSON.parse(result.stdout);
}

function prepare() {
  const version = validateReleaseIdentity({ rootDir: REPO_ROOT });
  const tauriDir = path.join(REPO_ROOT, "gui", "src-tauri");
  const popplerDir = path.join(tauriDir, "resources", "poppler");
  const provenancePath = path.join(popplerDir, "PROVENANCE.json");
  const releaseBuild = process.env.PIPELINE_RELEASE_BUILD === "1";
  if (releaseBuild && !fs.existsSync(provenancePath)) {
    throw new Error("release build is missing resources/poppler/PROVENANCE.json");
  }
  const provenance = fs.existsSync(provenancePath)
    ? JSON.parse(fs.readFileSync(provenancePath, "utf8"))
    : undefined;

  const output = path.join(tauriDir, "resources", "notices");
  fs.mkdirSync(output, { recursive: true });
  for (const generated of [
    "NOTICE.txt",
    "PIPELINE_LICENSE.txt",
    "POPPLER_INPUT_LOCK.json",
    "POPPLER_PROVENANCE.json",
    "THIRD_PARTY_LICENSES.md",
    "THIRD_PARTY_SBOM.cdx.json",
    "licenses",
  ]) {
    fs.rmSync(path.join(output, generated), { recursive: true, force: true });
  }
  fs.copyFileSync(path.join(REPO_ROOT, "LICENSE"), path.join(output, "PIPELINE_LICENSE.txt"));
  fs.copyFileSync(path.join(REPO_ROOT, "THIRD_PARTY_LICENSES.md"), path.join(output, "THIRD_PARTY_LICENSES.md"));
  fs.copyFileSync(path.join(REPO_ROOT, "scripts", "release", "poppler-lock.json"), path.join(output, "POPPLER_INPUT_LOCK.json"));
  if (provenance) {
    fs.copyFileSync(provenancePath, path.join(output, "POPPLER_PROVENANCE.json"));
    const popplerLicenses = path.join(popplerDir, "licenses");
    if (fs.existsSync(popplerLicenses)) {
      fs.cpSync(popplerLicenses, path.join(output, "licenses", "poppler"), { recursive: true });
    }
  }

  const metadata = cargoMetadata(tauriDir, releaseBuild);
  for (const item of metadata?.packages ?? []) {
    if (!item.source) continue;
    const sourceDir = path.dirname(item.manifest_path);
    const explicit = item.license_file
      ? path.resolve(sourceDir, item.license_file)
      : undefined;
    copyLicenseFiles(
      sourceDir,
      path.join(output, "licenses", "cargo", safeName(`${item.name}-${item.version}`)),
      explicit,
    );
  }

  const packageLock = JSON.parse(fs.readFileSync(path.join(REPO_ROOT, "gui", "package-lock.json"), "utf8"));
  for (const [lockPath, item] of Object.entries(packageLock.packages ?? {})) {
    if (!lockPath || !item?.version) continue;
    const name = npmName(lockPath, item);
    if (!name) continue;
    copyLicenseFiles(
      path.join(REPO_ROOT, "gui", lockPath),
      path.join(output, "licenses", "npm", safeName(`${name}-${item.version}`)),
    );
  }

  const sbom = buildSbom({
    version,
    cargoLock: fs.readFileSync(path.join(tauriDir, "Cargo.lock"), "utf8"),
    packageLock,
    provenance,
    cargoMetadata: metadata,
  });
  fs.writeFileSync(path.join(output, "THIRD_PARTY_SBOM.cdx.json"), `${JSON.stringify(sbom, null, 2)}\n`);
  fs.writeFileSync(
    path.join(output, "NOTICE.txt"),
    [
      `Pipeline ${version}`,
      "",
      "Pipeline's MIT license is in PIPELINE_LICENSE.txt.",
      "Third-party terms and source locations are in THIRD_PARTY_LICENSES.md.",
      "The machine-readable dependency inventory is in THIRD_PARTY_SBOM.cdx.json.",
      provenance
        ? "This platform's exact Poppler inputs and file hashes are in POPPLER_PROVENANCE.json."
        : "This development build does not contain a release Poppler provenance manifest.",
      "Corresponding Poppler license texts are under licenses/poppler/ when Poppler is bundled.",
      "",
    ].join("\n"),
  );
  console.log(`Prepared offline notices and ${sbom.components.length} SBOM components in ${output}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    prepare();
  } catch (error) {
    console.error(`Notice preparation failed: ${error.message}`);
    process.exitCode = 1;
  }
}
