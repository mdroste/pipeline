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
    const installScope = entry.dev || entry.devOptional ? "development" : "production";
    const existing = byReference.get(purl);
    if (existing) {
      // A single package/version can occur under both a development-only path
      // and a production path. The SBOM is deduplicated by purl, so retain the
      // stronger production classification when either occurrence ships.
      const scope = existing.properties.find(
        (property) => property.name === "pipeline:npm-install-scope",
      );
      if (installScope === "production") scope.value = "production";
      continue;
    }
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
      properties: [{
        name: "pipeline:npm-install-scope",
        value: installScope,
      }],
    };
    if (typeof entry.license === "string" && entry.license.trim()) {
      component.licenses = [{ license: { name: entry.license.trim() } }];
    }
    byReference.set(purl, component);
  }
  return [...byReference.values()];
}

function normalizePythonName(name) {
  return name.toLowerCase().replace(/[-_.]+/g, "-");
}

export function pythonRuntimeComponents({ locks = {}, runtimeLock, licenseInventory } = {}) {
  if (!runtimeLock) return [];
  const licenses = new Map(
    (licenseInventory?.packages ?? []).map((item) => [
      `${normalizePythonName(item.name)}@${item.version}`,
      item.declaredLicense,
    ]),
  );
  const packages = new Map();
  for (const [platform, contents] of Object.entries(locks)) {
    for (const block of contents.split("[[packages]]").slice(1)) {
      const name = block.match(/^\s*name = "([^"]+)"$/m)?.[1];
      const version = block.match(/^version = "([^"]+)"$/m)?.[1];
      if (!name || !version) continue;
      const normalized = normalizePythonName(name);
      const purl = `pkg:pypi/${encodeURIComponent(normalized)}@${encodeURIComponent(version)}`;
      const existing = packages.get(purl) ?? {
        type: "library",
        "bom-ref": purl,
        group: "pypi",
        name: normalized,
        version,
        purl,
        platforms: new Set(),
        wheels: new Map(),
      };
      existing.platforms.add(platform);
      for (const match of block.matchAll(/url = "([^"]+)"[^}\]]*?sha256 = "([a-f0-9]{64})"/gs)) {
        existing.wheels.set(match[1], match[2]);
      }
      packages.set(purl, existing);
    }
  }

  const components = [...packages.values()].map((item) => {
    const component = {
      type: item.type,
      "bom-ref": item["bom-ref"],
      group: item.group,
      name: item.name,
      version: item.version,
      purl: item.purl,
      externalReferences: [...item.wheels.keys()]
        .sort()
        .map((url) => ({ type: "distribution", url })),
      properties: [
        { name: "pipeline:optional-managed-runtime", value: "paddleocr-vl-parser" },
        { name: "pipeline:platforms", value: [...item.platforms].sort().join(",") },
        ...[...item.wheels.entries()]
          .sort(([left], [right]) => left.localeCompare(right))
          .map(([url, digest]) => ({
            name: "pipeline:wheel-sha256",
            value: `${digest} ${url}`,
          })),
      ],
    };
    const declaredLicense = licenses.get(`${item.name}@${item.version}`);
    if (declaredLicense) component.licenses = [{ license: { name: declaredLicense } }];
    return component;
  });

  for (const [platform, artifact] of Object.entries(runtimeLock.python.artifacts)) {
    const url = "https://releases.astral.sh/github/python-build-standalone/releases/download/"
      + `${runtimeLock.python.buildRelease}/cpython-${runtimeLock.python.version}%2B`
      + `${runtimeLock.python.buildRelease}-${artifact.target}-install_only_stripped.tar.gz`;
    const purl = `pkg:generic/cpython@${encodeURIComponent(runtimeLock.python.version)}?platform=${encodeURIComponent(platform)}`;
    components.push({
      type: "framework",
      "bom-ref": purl,
      name: "CPython",
      version: runtimeLock.python.version,
      purl,
      hashes: componentHash("SHA-256", artifact.sha256),
      licenses: [{ license: { name: runtimeLock.python.license } }],
      externalReferences: [{ type: "distribution", url }],
      properties: [{ name: "pipeline:optional-managed-runtime", value: "paddleocr-vl-parser" }],
    });
    components.push({
      type: "file",
      "bom-ref": `pipeline:paddle-parser-lock:${platform}`,
      name: artifact.lock,
      hashes: componentHash("SHA-256", artifact.lockSha256),
      properties: [{ name: "pipeline:platform", value: platform }],
    });
  }

  for (const [platform, artifact] of Object.entries(runtimeLock.uv.artifacts)) {
    const extension = platform.startsWith("windows-") ? ".zip" : ".tar.gz";
    const url = `https://github.com/astral-sh/uv/releases/download/${runtimeLock.uv.version}/uv-${artifact.target}${extension}`;
    const purl = `pkg:generic/astral-sh/uv@${encodeURIComponent(runtimeLock.uv.version)}?platform=${encodeURIComponent(platform)}`;
    components.push({
      type: "application",
      "bom-ref": purl,
      name: "uv",
      version: runtimeLock.uv.version,
      purl,
      hashes: componentHash("SHA-256", artifact.sha256),
      licenses: [{ license: { name: runtimeLock.uv.license } }],
      externalReferences: [{ type: "distribution", url }],
      properties: [{ name: "pipeline:optional-managed-runtime", value: "paddleocr-vl-parser" }],
    });
  }

  const model = runtimeLock.layoutModel;
  const modelPurl = `pkg:generic/PaddlePaddle/${encodeURIComponent(model.name)}@${encodeURIComponent(model.version)}`;
  components.push({
    type: "machine-learning-model",
    "bom-ref": modelPurl,
    name: model.name,
    version: model.version,
    purl: modelPurl,
    hashes: componentHash("SHA-256", model.sha256),
    licenses: [{ license: { name: model.license } }],
    externalReferences: [{ type: "distribution", url: model.url }],
    properties: [{ name: "pipeline:optional-managed-runtime", value: "paddleocr-vl-parser" }],
  });
  return components;
}

function nativePackagePurl(provenance, item) {
  if (!item.version) return undefined;
  const manager = provenance.packageInventory?.manager;
  if (manager === "homebrew") {
    return `pkg:homebrew/${encodeURIComponent(item.name)}@${encodeURIComponent(item.version)}`;
  }
  if (manager === "dpkg") {
    const [name, architecture] = item.name.split(":", 2);
    const qualifiers = new URLSearchParams({
      distro: "ubuntu-22.04",
      ...(architecture ? { arch: architecture } : {}),
    });
    return `pkg:deb/ubuntu/${encodeURIComponent(name)}@${encodeURIComponent(item.version)}?${qualifiers}`;
  }
  if (manager === "conda") {
    const qualifiers = new URLSearchParams({
      build: item.build,
      channel: provenance.packageInventory.channel,
      name: item.name,
      subdir: provenance.packageInventory.subdir,
    });
    // Anchore represents CondaPkg as pkg:generic/conda; keeping the real
    // package name in both the CycloneDX component and a qualifier lets Grype
    // decode the component as a conda package without losing its identity.
    return `pkg:generic/conda@${encodeURIComponent(item.version)}?${qualifiers}`;
  }
  return undefined;
}

export function popplerComponents(provenance) {
  if (!provenance) return [];
  const popplerPurl = `pkg:generic/poppler@${encodeURIComponent(provenance.version)}?platform=${provenance.platform}`;
  const components = [{
    type: "application",
    "bom-ref": popplerPurl,
    name: "poppler command-line utilities",
    version: provenance.version,
    purl: popplerPurl,
    licenses: [{ expression: provenance.license }],
    externalReferences: [{ type: "distribution", url: provenance.source.url }],
    properties: [
      { name: "pipeline:binary-provider", value: provenance.binaryProvider },
      { name: "pipeline:platform", value: provenance.platform },
    ],
  }];

  for (const item of provenance.packageInventory?.packages ?? []) {
    if (!item.name) continue;
    const purl = nativePackagePurl(provenance, item);
    const reference = purl
      ?? `pipeline:system-package:${provenance.platform}:${item.name}@${item.version ?? "unknown"}`;
    const component = {
      type: "library",
      "bom-ref": reference,
      name: item.name,
      properties: [
        { name: "pipeline:system-package", value: "true" },
        { name: "pipeline:platform", value: provenance.platform },
        {
          name: "pipeline:native-package-manager",
          value: provenance.packageInventory.manager ?? "system",
        },
        {
          name: "pipeline:license-policy",
          value: "declared metadata or bundled package/archive notice; human review required",
        },
      ],
    };
    // Do not set CycloneDX `group`: Syft prefixes it onto Homebrew/Conda
    // package names, which would make Grype query `homebrew/pkg` or
    // `conda/pkg` instead of the real native package name.
    if (item.version) component.version = item.version;
    if (purl) component.purl = purl;
    if (item.archiveSha256) {
      component.hashes = componentHash("SHA-256", item.archiveSha256);
      component.properties.push({
        name: "pipeline:hash-subject",
        value: "source package archive",
      });
    }
    if (provenance.packageInventory.manager === "conda") {
      component.properties.push(
        { name: "syft:package:type", value: "conda" },
        { name: "pipeline:conda-build", value: item.build },
      );
    }
    if (typeof item.license === "string" && item.license) {
      component.licenses = [{ license: { name: item.license } }];
    }
    if (item.sourceUrl) {
      component.externalReferences = [{ type: "distribution", url: item.sourceUrl }];
    }
    if (item.cpe) component.cpe = item.cpe;
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

export function buildSbom({
  version,
  cargoLock,
  packageLock,
  provenance,
  cargoMetadata,
  parserRuntime,
}) {
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
    ...pythonRuntimeComponents(parserRuntime),
  ].sort((a, b) => a["bom-ref"].localeCompare(b["bom-ref"]));
  const componentReferences = components.map((component) => component["bom-ref"]);
  if (new Set(componentReferences).size !== componentReferences.length) {
    throw new Error("build-input SBOM contains duplicate component bom-refs");
  }

  const identity = crypto
    .createHash("sha256")
    .update(JSON.stringify({ version, refs: components.map((item) => item["bom-ref"]) }))
    .digest("hex");
  const uuid = `${identity.slice(0, 8)}-${identity.slice(8, 12)}-5${identity.slice(13, 16)}-a${identity.slice(17, 20)}-${identity.slice(20, 32)}`;
  const rootReference = `pkg:github/mdroste/pipeline@${encodeURIComponent(version)}`;
  return {
    bomFormat: "CycloneDX",
    specVersion: "1.5",
    serialNumber: `urn:uuid:${uuid}`,
    version: 1,
    metadata: {
      component: {
        type: "application",
        "bom-ref": rootReference,
        name: "Pipeline",
        version,
        licenses: [{ license: { id: "MIT" } }],
      },
      properties: [
        { name: "pipeline:scope", value: "locked Rust, npm, platform Poppler, and optional Paddle parser release inputs" },
        { name: "pipeline:generator", value: "scripts/release/prepare-notices.mjs" },
        { name: "pipeline:platform", value: provenance?.platform ?? "development" },
      ],
    },
    components,
    dependencies: [
      { ref: rootReference, dependsOn: componentReferences },
      ...componentReferences.map((ref) => ({ ref, dependsOn: [] })),
    ],
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
      if (!entry.isFile()) continue;
      const candidate = path.join(sourceDir, entry.name);
      if (/^(copying|copyright|licen[cs]e|notice)(?:[._-].*)?$/i.test(entry.name)) {
        candidates.push(candidate);
      } else if (/^readme(?:[._-].*)?$/i.test(entry.name) && fs.statSync(candidate).size <= 2 * 1024 * 1024) {
        // Some older npm packages put the complete license only in README.
        // Treat it as evidence only when it has a license heading and
        // recognizable grant text, rather than copying arbitrary readmes.
        const readme = fs.readFileSync(candidate, "utf8");
        if (
          /^#{1,4}\s+licen[cs]e\b/im.test(readme)
          && /(permission is hereby granted|redistribution and use|apache license|gnu (?:general|lesser) public license)/i.test(readme)
        ) {
          candidates.push(candidate);
        }
      }
    }
  }
  const unique = [...new Set(candidates.map((file) => path.resolve(file)))];
  const copied = [];
  for (const file of unique) {
    if (fs.statSync(file).size > 2 * 1024 * 1024) continue;
    fs.mkdirSync(destination, { recursive: true });
    const target = path.join(destination, path.basename(file));
    fs.copyFileSync(file, target);
    copied.push(target);
  }
  return copied;
}

export function validateLicenseInventory(entries, required = false) {
  const normalized = [...entries]
    .map((entry) => ({
      ecosystem: entry.ecosystem,
      name: entry.name,
      version: entry.version,
      declaredLicense: entry.declaredLicense || null,
      licenseFiles: [...new Set(entry.licenseFiles ?? [])].sort(),
    }))
    .sort((left, right) => (
      `${left.ecosystem}:${left.name}@${left.version}`
        .localeCompare(`${right.ecosystem}:${right.name}@${right.version}`)
    ));
  const incomplete = normalized.filter(
    (entry) => !entry.declaredLicense && entry.licenseFiles.length === 0,
  );
  if (required && incomplete.length) {
    throw new Error(
      `third-party packages have neither a declared license nor an offline license file: ${
        incomplete.map((entry) => `${entry.ecosystem}:${entry.name}@${entry.version}`).join(", ")
      }`,
    );
  }
  return {
    schemaVersion: 1,
    complete: incomplete.length === 0,
    packages: normalized,
    incomplete: incomplete.map(
      (entry) => `${entry.ecosystem}:${entry.name}@${entry.version}`,
    ),
  };
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
  const parserDir = path.join(tauriDir, "resources", "paddle-parser");
  const parserRuntimeLock = JSON.parse(
    fs.readFileSync(path.join(parserDir, "runtime-lock.json"), "utf8"),
  );
  const parserLicenseInventory = JSON.parse(
    fs.readFileSync(path.join(parserDir, "python-licenses.json"), "utf8"),
  );
  const parserLocks = Object.fromEntries(
    Object.entries(parserRuntimeLock.python.artifacts).map(([platform, artifact]) => [
      platform,
      fs.readFileSync(path.join(parserDir, artifact.lock), "utf8"),
    ]),
  );
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
    "LICENSE_INVENTORY.json",
    "PADDLE_PARSER_LICENSE_INVENTORY.json",
    "PADDLE_PARSER_RUNTIME_LOCK.json",
    "paddle-parser-locks",
    "licenses",
  ]) {
    fs.rmSync(path.join(output, generated), { recursive: true, force: true });
  }
  fs.copyFileSync(path.join(REPO_ROOT, "LICENSE"), path.join(output, "PIPELINE_LICENSE.txt"));
  fs.copyFileSync(path.join(REPO_ROOT, "THIRD_PARTY_LICENSES.md"), path.join(output, "THIRD_PARTY_LICENSES.md"));
  fs.copyFileSync(path.join(REPO_ROOT, "scripts", "release", "poppler-lock.json"), path.join(output, "POPPLER_INPUT_LOCK.json"));
  fs.copyFileSync(
    path.join(parserDir, "runtime-lock.json"),
    path.join(output, "PADDLE_PARSER_RUNTIME_LOCK.json"),
  );
  fs.mkdirSync(path.join(output, "paddle-parser-locks"), { recursive: true });
  for (const artifact of Object.values(parserRuntimeLock.python.artifacts)) {
    fs.copyFileSync(
      path.join(parserDir, artifact.lock),
      path.join(output, "paddle-parser-locks", artifact.lock),
    );
  }
  if (provenance) {
    fs.copyFileSync(provenancePath, path.join(output, "POPPLER_PROVENANCE.json"));
    const popplerLicenses = path.join(popplerDir, "licenses");
    if (fs.existsSync(popplerLicenses)) {
      fs.cpSync(popplerLicenses, path.join(output, "licenses", "poppler"), { recursive: true });
    }
  }

  const metadata = cargoMetadata(tauriDir, releaseBuild);
  const licenseEntries = [];
  for (const item of metadata?.packages ?? []) {
    if (!item.source) continue;
    const sourceDir = path.dirname(item.manifest_path);
    const explicit = item.license_file
      ? path.resolve(sourceDir, item.license_file)
      : undefined;
    const copied = copyLicenseFiles(
      sourceDir,
      path.join(output, "licenses", "cargo", safeName(`${item.name}-${item.version}`)),
      explicit,
    );
    licenseEntries.push({
      ecosystem: "cargo",
      name: item.name,
      version: item.version,
      declaredLicense: item.license,
      licenseFiles: copied.map((file) => path.relative(output, file)),
    });
  }

  const packageLock = JSON.parse(fs.readFileSync(path.join(REPO_ROOT, "gui", "package-lock.json"), "utf8"));
  const npmLicenseEntries = new Map();
  for (const [lockPath, item] of Object.entries(packageLock.packages ?? {})) {
    if (!lockPath || !item?.version) continue;
    const name = npmName(lockPath, item);
    if (!name) continue;
    const copied = copyLicenseFiles(
      path.join(REPO_ROOT, "gui", lockPath),
      path.join(output, "licenses", "npm", safeName(`${name}-${item.version}`)),
    );
    const key = `${name}@${item.version}`;
    const existing = npmLicenseEntries.get(key) ?? {
      ecosystem: "npm",
      name,
      version: item.version,
      declaredLicense: item.license,
      licenseFiles: [],
    };
    existing.declaredLicense ||= item.license;
    existing.licenseFiles.push(...copied.map((file) => path.relative(output, file)));
    npmLicenseEntries.set(key, existing);
  }
  licenseEntries.push(...npmLicenseEntries.values());
  const licenseInventory = validateLicenseInventory(licenseEntries, releaseBuild);
  fs.writeFileSync(
    path.join(output, "LICENSE_INVENTORY.json"),
    `${JSON.stringify(licenseInventory, null, 2)}\n`,
  );

  const parserLicenseEntries = [
    ...parserLicenseInventory.packages.map((item) => ({
      ecosystem: "pypi",
      name: normalizePythonName(item.name),
      version: item.version,
      declaredLicense: item.declaredLicense,
      licenseFiles: [],
    })),
    {
      ecosystem: "managed-runtime",
      name: "CPython",
      version: parserRuntimeLock.python.version,
      declaredLicense: parserRuntimeLock.python.license,
      licenseFiles: [],
    },
    {
      ecosystem: "managed-runtime",
      name: "uv",
      version: parserRuntimeLock.uv.version,
      declaredLicense: parserRuntimeLock.uv.license,
      licenseFiles: [],
    },
    {
      ecosystem: "managed-model",
      name: parserRuntimeLock.layoutModel.name,
      version: parserRuntimeLock.layoutModel.version,
      declaredLicense: parserRuntimeLock.layoutModel.license,
      licenseFiles: [],
    },
  ];
  const parserLicenseEvidence = validateLicenseInventory(parserLicenseEntries, true);
  const lockedPythonPackages = new Set(
    pythonRuntimeComponents({
      locks: parserLocks,
      runtimeLock: parserRuntimeLock,
      licenseInventory: parserLicenseInventory,
    })
      .filter((item) => item.group === "pypi")
      .map((item) => `${item.name}@${item.version}`),
  );
  const licensedPythonPackages = new Set(
    parserLicenseEntries
      .filter((item) => item.ecosystem === "pypi")
      .map((item) => `${item.name}@${item.version}`),
  );
  const missingLicenseEvidence = [...lockedPythonPackages]
    .filter((item) => !licensedPythonPackages.has(item));
  const staleLicenseEvidence = [...licensedPythonPackages]
    .filter((item) => !lockedPythonPackages.has(item));
  if (missingLicenseEvidence.length || staleLicenseEvidence.length) {
    throw new Error(
      `Paddle parser license evidence does not match its locks (missing: ${missingLicenseEvidence.join(", ") || "none"}; stale: ${staleLicenseEvidence.join(", ") || "none"})`,
    );
  }
  fs.writeFileSync(
    path.join(output, "PADDLE_PARSER_LICENSE_INVENTORY.json"),
    `${JSON.stringify(parserLicenseEvidence, null, 2)}\n`,
  );

  const sbom = buildSbom({
    version,
    cargoLock: fs.readFileSync(path.join(tauriDir, "Cargo.lock"), "utf8"),
    packageLock,
    provenance,
    cargoMetadata: metadata,
    parserRuntime: {
      locks: parserLocks,
      runtimeLock: parserRuntimeLock,
      licenseInventory: parserLicenseInventory,
    },
  });
  fs.writeFileSync(path.join(output, "THIRD_PARTY_SBOM.cdx.json"), `${JSON.stringify(sbom, null, 2)}\n`);
  fs.writeFileSync(
    path.join(output, "NOTICE.txt"),
    [
      `Pipeline ${version}`,
      "",
      "Pipeline's MIT license is in PIPELINE_LICENSE.txt.",
      "Third-party terms and source locations are in THIRD_PARTY_LICENSES.md.",
      "The machine-readable build-input dependency inventory is in THIRD_PARTY_SBOM.cdx.json.",
      "Declared licenses and copied offline license files are audited in LICENSE_INVENTORY.json.",
      "The optional managed Paddle parser's locks and licenses are in PADDLE_PARSER_RUNTIME_LOCK.json, paddle-parser-locks/, and PADDLE_PARSER_LICENSE_INVENTORY.json.",
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
