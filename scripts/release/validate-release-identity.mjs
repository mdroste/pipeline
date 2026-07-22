#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const REPO_ROOT = path.resolve(path.dirname(SCRIPT_PATH), "../..");
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function cargoPackageVersion(contents) {
  const packageBlock = contents.match(/(?:^|\n)\[package\]\s*\n([\s\S]*?)(?=\n\[|$)/);
  const version = packageBlock?.[1].match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version) throw new Error("gui/src-tauri/Cargo.toml has no [package] version");
  return version;
}

function cargoLockPackageVersion(contents, name) {
  for (const block of contents.split(/(?=^\[\[package\]\]\s*$)/m)) {
    if (block.match(/^name\s*=\s*"([^"]+)"\s*$/m)?.[1] !== name) continue;
    const version = block.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
    if (version) return version;
  }
  throw new Error(`gui/src-tauri/Cargo.lock has no ${name} package entry`);
}

export function validateReleaseIdentity({ rootDir = REPO_ROOT, tag } = {}) {
  const guiDir = path.join(rootDir, "gui");
  const packageJson = readJson(path.join(guiDir, "package.json"));
  const packageLock = readJson(path.join(guiDir, "package-lock.json"));
  const tauriConfig = readJson(path.join(guiDir, "src-tauri", "tauri.conf.json"));
  const popplerLock = readJson(path.join(rootDir, "scripts", "release", "poppler-lock.json"));
  const cargoToml = fs.readFileSync(path.join(guiDir, "src-tauri", "Cargo.toml"), "utf8");
  const cargoLock = fs.readFileSync(path.join(guiDir, "src-tauri", "Cargo.lock"), "utf8");

  const version = packageJson.version;
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new Error(`gui/package.json version is not valid SemVer: ${JSON.stringify(version)}`);
  }

  const checks = [
    ["gui/package-lock.json top-level version", packageLock.version],
    ["gui/package-lock.json root package version", packageLock.packages?.[""]?.version],
    ["gui/src-tauri/Cargo.toml package version", cargoPackageVersion(cargoToml)],
    ["gui/src-tauri/Cargo.lock pipeline-gui version", cargoLockPackageVersion(cargoLock, "pipeline-gui")],
  ];
  for (const [label, actual] of checks) {
    if (actual !== version) {
      throw new Error(`${label} is ${JSON.stringify(actual)}; canonical gui/package.json is ${version}`);
    }
  }

  // Tauri accepts a path to a package.json as its version source. Keeping this
  // as a path (rather than another literal) removes one release identity copy.
  if (tauriConfig.version !== "../package.json") {
    throw new Error(
      `gui/src-tauri/tauri.conf.json version must be "../package.json", got ${JSON.stringify(tauriConfig.version)}`,
    );
  }
  if (tauriConfig.bundle?.macOS?.minimumSystemVersion !== popplerLock.macos?.deploymentTarget) {
    throw new Error(
      `Tauri macOS minimum ${JSON.stringify(tauriConfig.bundle?.macOS?.minimumSystemVersion)} must equal the Poppler closure target ${JSON.stringify(popplerLock.macos?.deploymentTarget)}`,
    );
  }
  for (const resource of ["resources/poppler/**/*", "resources/notices/**/*"]) {
    if (!tauriConfig.bundle?.resources?.includes(resource)) {
      throw new Error(`Tauri release resources must include ${resource}`);
    }
  }

  if (tag !== undefined && tag !== `v${version}`) {
    throw new Error(`release tag ${JSON.stringify(tag)} must exactly equal v${version}`);
  }

  return version;
}

function parseArgs(argv) {
  const parsed = { tag: process.env.GITHUB_REF_TYPE === "tag" ? process.env.GITHUB_REF_NAME : undefined };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--tag") parsed.tag = argv[++i];
    else if (argv[i] === "--github-output") parsed.githubOutput = argv[++i];
    else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return parsed;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    const options = parseArgs(process.argv.slice(2));
    const version = validateReleaseIdentity({ tag: options.tag });
    if (options.githubOutput) fs.appendFileSync(options.githubOutput, `num=${version}\n`);
    console.log(`Release identity validated: Pipeline ${version}`);
  } catch (error) {
    console.error(`Release identity validation failed: ${error.message}`);
    process.exitCode = 1;
  }
}
