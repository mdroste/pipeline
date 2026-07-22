#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const SCRIPT_DIR = path.dirname(SCRIPT_PATH);
const LOCK_PATH = path.join(SCRIPT_DIR, "poppler-lock.json");
const MANIFEST_NAME = "PROVENANCE.json";

export function sha256File(file) {
  return crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
}

export function collectFiles(rootDir) {
  const files = [];
  function walk(dir) {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const absolute = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(absolute);
      else if (entry.isFile() && entry.name !== MANIFEST_NAME && entry.name !== ".gitkeep") {
        const relative = path.relative(rootDir, absolute).split(path.sep).join("/");
        files.push({ path: relative, size: fs.statSync(absolute).size, sha256: sha256File(absolute) });
      }
    }
  }
  walk(rootDir);
  return files.sort((a, b) => a.path.localeCompare(b.path));
}

export function verifyFileSet(rootDir, expectedFiles) {
  const actual = collectFiles(rootDir);
  if (JSON.stringify(actual) !== JSON.stringify(expectedFiles)) {
    const expectedByPath = new Map(expectedFiles.map((file) => [file.path, file]));
    const actualByPath = new Map(actual.map((file) => [file.path, file]));
    const differences = [];
    for (const name of new Set([...expectedByPath.keys(), ...actualByPath.keys()])) {
      if (JSON.stringify(expectedByPath.get(name)) !== JSON.stringify(actualByPath.get(name))) differences.push(name);
    }
    throw new Error(`bundled Poppler files do not match provenance: ${differences.sort().join(", ")}`);
  }
}

function parseArgs(argv) {
  const options = { mode: "verify" };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--create") options.mode = "create";
    else if (arg === "--verify") options.mode = "verify";
    else if (arg === "--platform") options.platform = argv[++i];
    else if (arg === "--resource-dir") options.resourceDir = path.resolve(argv[++i]);
    else if (arg === "--package-inventory") options.packageInventory = path.resolve(argv[++i]);
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!options.platform || !["macos", "linux", "windows"].includes(options.platform)) {
    throw new Error("--platform must be macos, linux, or windows");
  }
  if (!options.resourceDir) throw new Error("--resource-dir is required");
  options.packageInventory ??= path.join(options.resourceDir, "package-inventory.json");
  return options;
}

function bundledVersion(resourceDir, platform) {
  const executable = path.join(resourceDir, platform === "windows" ? "pdftotext.exe" : "pdftotext");
  if (!fs.existsSync(executable)) throw new Error(`missing bundled executable: ${executable}`);
  const result = spawnSync(executable, ["-v"], { encoding: "utf8", timeout: 30_000 });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`pdftotext -v exited ${result.status}: ${(result.stderr || result.stdout).trim()}`);
  }
  const output = `${result.stdout}\n${result.stderr}`;
  const version = output.match(/(?:pdftotext version\s+)?(\d+\.\d+\.\d+)/i)?.[1];
  if (!version) throw new Error(`could not parse bundled pdftotext version from: ${output.trim()}`);
  return version;
}

function validateRequiredOfflineFiles(resourceDir, packageInventory) {
  const licenseDir = path.join(resourceDir, "licenses");
  const licenseFiles = fs.existsSync(licenseDir)
    ? collectFiles(licenseDir).filter((file) => file.size > 0)
    : [];
  if (licenseFiles.length === 0) throw new Error("bundled Poppler has no offline license files");
  if (!fs.existsSync(path.join(resourceDir, "share", "poppler"))) {
    throw new Error("bundled Poppler data directory share/poppler is missing");
  }
  if (!packageInventory || !Array.isArray(packageInventory.packages) || packageInventory.packages.length === 0) {
    throw new Error("package-inventory.json must contain a non-empty packages array");
  }
}

function createManifest(options) {
  const lockBytes = fs.readFileSync(LOCK_PATH);
  const lock = JSON.parse(lockBytes);
  const platformLock = lock[options.platform];
  if (!platformLock) throw new Error(`poppler-lock.json has no ${options.platform} entry`);
  const version = bundledVersion(options.resourceDir, options.platform);
  if (version !== platformLock.version) {
    throw new Error(`bundled Poppler is ${version}; lock requires ${platformLock.version}`);
  }
  const packageInventory = JSON.parse(fs.readFileSync(options.packageInventory, "utf8"));
  validateRequiredOfflineFiles(options.resourceDir, packageInventory);
  return {
    schemaVersion: 1,
    component: "poppler command-line utilities",
    platform: options.platform,
    version,
    license: platformLock.license,
    binaryProvider: platformLock.binaryProvider,
    source: platformLock.source,
    binaryInput: platformLock.archive ?? {
      packageVersion: platformLock.packageVersion ?? version,
      homebrewCoreCommit: platformLock.homebrewCoreCommit,
      distribution: platformLock.distribution,
      patchedSource: platformLock.patchedSource,
    },
    inputLockSha256: crypto.createHash("sha256").update(lockBytes).digest("hex"),
    packageInventory,
    files: collectFiles(options.resourceDir),
  };
}

function run(options) {
  const manifestPath = path.join(options.resourceDir, MANIFEST_NAME);
  if (options.mode === "create") {
    const manifest = createManifest(options);
    fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    console.log(`Wrote ${manifestPath} (${manifest.files.length} bundled files)`);
    return;
  }

  if (!fs.existsSync(manifestPath)) throw new Error(`missing ${manifestPath}`);
  const expected = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const actual = createManifest(options);
  for (const field of [
    "component",
    "platform",
    "version",
    "license",
    "binaryProvider",
    "source",
    "binaryInput",
    "inputLockSha256",
  ]) {
    if (JSON.stringify(expected[field]) !== JSON.stringify(actual[field])) {
      throw new Error(`Poppler provenance ${field} mismatch`);
    }
  }
  if (JSON.stringify(expected.packageInventory) !== JSON.stringify(actual.packageInventory)) {
    throw new Error("Poppler package inventory no longer matches provenance");
  }
  verifyFileSet(options.resourceDir, expected.files);
  console.log(`Verified Poppler ${actual.version} provenance and ${actual.files.length} bundled files`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    run(parseArgs(process.argv.slice(2)));
  } catch (error) {
    console.error(`Poppler provenance validation failed: ${error.message}`);
    process.exitCode = 1;
  }
}
