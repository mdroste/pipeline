#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import { installerNames } from "./draft-release.mjs";
import { updatePackages } from "./update-manifest.mjs";

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const STABLE_VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function expectedReleaseAssets(version) {
  if (!STABLE_VERSION.test(version)) {
    throw new Error(`release completeness requires a stable SemVer version, got ${JSON.stringify(version)}`);
  }
  const updates = updatePackages(version).map(({ name }) => name);
  const checksummed = [...new Set([...installerNames(version), ...updates])];
  return [
    ...checksummed.flatMap((name) => [name, `${name}.sha256`]),
    ...updates.map((name) => `${name}.sig`),
    "latest.json",
  ].sort();
}

export function validateReleaseAssets({ version, assetNames }) {
  if (!Array.isArray(assetNames) || assetNames.some((name) => typeof name !== "string")) {
    throw new Error("release asset inventory must be an array of names");
  }
  const duplicates = [...new Set(assetNames.filter((name, index) => assetNames.indexOf(name) !== index))];
  if (duplicates.length) {
    throw new Error(`release contains duplicate asset names: ${duplicates.sort().join(", ")}`);
  }
  const available = new Set(assetNames);
  const expected = expectedReleaseAssets(version);
  const missing = expected.filter((name) => !available.has(name));
  if (missing.length) {
    throw new Error(`release is incomplete; missing required assets: ${missing.join(", ")}`);
  }
  const unexpected = assetNames.filter((name) => !expected.includes(name));
  if (unexpected.length) {
    throw new Error(`release contains unexpected assets: ${unexpected.sort().join(", ")}`);
  }
  return expected;
}

function parseArgs(argv) {
  const options = {};
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--version") options.version = argv[++index];
    else if (argument === "--release-json") options.releaseJson = argv[++index];
    else throw new Error(`unknown argument: ${argument}`);
  }
  if (!options.version) throw new Error("--version is required");
  if (!options.releaseJson) throw new Error("--release-json is required");
  return options;
}

if (process.argv[1] && path.resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    const options = parseArgs(process.argv.slice(2));
    const release = JSON.parse(fs.readFileSync(options.releaseJson, "utf8"));
    if (!release || release.isDraft !== true) {
      throw new Error("release completeness must run against the draft release");
    }
    const validated = validateReleaseAssets({
      version: options.version,
      assetNames: (release.assets ?? []).map((asset) => asset.name),
    });
    console.log(`Release completeness verified: ${validated.length} required assets`);
  } catch (error) {
    console.error(`Release completeness validation failed: ${error.message}`);
    process.exitCode = 1;
  }
}
