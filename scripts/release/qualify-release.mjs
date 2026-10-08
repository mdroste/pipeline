#!/usr/bin/env node
// A candidate-specific evidence gate. It validates recorded evidence, not the
// truth of human observations; reviewers must inspect the referenced records.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
export const requiredScenarios = ["shared-account", "workspace-native-tools", "review-providers", "automations", "research-tools", "crash-storage", "performance-accessibility", "dependencies-notices"];
export const requiredPlatforms = ["macos-arm64", "macos-x64", "windows", "linux"];
const sha256 = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
export function qualify(record, commit, root) {
  if (record.schemaVersion !== 1 || !/^[a-f0-9]{40}$/.test(commit) || record.commit !== commit)
    throw new Error("Qualification must identify the exact final candidate commit");
  if (!/^\d+\.\d+\.\d+$/.test(record.version || "")) throw new Error("Candidate version is missing");
  const check = row => {
    if (!row || row.status !== "passed" || !row.observed?.trim() || !row.expected?.trim() || !row.environment?.trim() || !row.versions?.trim())
      throw new Error("Missing passed observation, expected outcome, environment, or tool/provider versions");
    if (!row.evidence?.length) throw new Error("Durable qualification evidence is required");
    for (const file of row.evidence) {
      if (path.isAbsolute(file.path) || file.path.split(/[\\/]/).includes("..") || !file.path)
        throw new Error("Evidence paths must be relative to the evidence folder");
      if (sha256(fs.readFileSync(path.join(root, file.path))) !== file.sha256)
        throw new Error(`Evidence checksum mismatch: ${file.path}`);
    }
  };
  for (const name of requiredScenarios) check(record.scenarios?.[name]);
  for (const platform of requiredPlatforms) {
    const row = record.platforms?.[platform]; check(row);
    if (!/^[a-f0-9]{64}$/.test(row.installerSha256 || "")) throw new Error(`Installer checksum required: ${platform}`);
    if (!row.installer || path.basename(row.installer) !== row.installer) throw new Error("Expected installer basename");
    if (sha256(fs.readFileSync(path.join(root, row.installer))) !== row.installerSha256)
      throw new Error(`Installer checksum mismatch: ${platform}`);
  }
  return true;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [manifest, commit] = process.argv.slice(2);
    if (!manifest || !commit) throw new Error("Usage: qualify-release.mjs <evidence.json> <candidate-commit>");
    qualify(JSON.parse(fs.readFileSync(manifest, "utf8")), commit, path.dirname(path.resolve(manifest)));
    console.log("Recorded candidate qualification passed; review observations before publishing.");
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
