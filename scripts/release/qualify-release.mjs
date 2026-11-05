#!/usr/bin/env node
// A candidate-specific evidence gate. It validates recorded evidence, not the
// truth of human observations; reviewers must inspect the referenced records.
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { installerNames } from "./draft-release.mjs";
export const requiredScenarios = ["shared-account", "workspace-native-tools", "review-providers", "automations", "research-tools", "crash-storage", "performance-accessibility", "dependencies-notices"];
export const requiredPlatforms = ["macos-arm64", "macos-x64", "windows", "linux"];
const sha256 = bytes => crypto.createHash("sha256").update(bytes).digest("hex");
function candidateNames(candidate) {
  if (candidate?.schemaVersion !== 1 || !/^[a-f0-9]{40}$/.test(candidate.commit || ""))
    throw new Error("Candidate manifest requires the exact final candidate commit");
  if (!/^\d+\.\d+\.\d+$/.test(candidate.version || "")) throw new Error("Candidate version is missing");
  return Object.fromEntries(requiredPlatforms.map((platform, i) => [platform, installerNames(candidate.version)[i]]));
}
// Evidence names a commit by hand, so hold it to what the signed tag points at.
export function verifyTaggedCommit(commit, version, git = args => execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] })) {
  let tagged;
  try { tagged = git(["rev-parse", "--verify", `refs/tags/v${version}^{commit}`]).trim(); }
  catch { throw new Error(`Tag v${version} isn't in this repository; fetch it first`); }
  if (tagged !== commit) throw new Error(`Tag v${version} points at ${tagged}, not candidate commit ${commit}`);
}
// Capture this independently from the verified draft's final installer bundle,
// before filling in qualification observations. Evidence cannot choose assets.
export function captureCandidate(commit, version, root) {
  const candidate = { schemaVersion: 1, commit, version, installers: {} };
  for (const [platform, name] of Object.entries(candidateNames(candidate))) {
    const hash = sha256(fs.readFileSync(path.join(root, name)));
    const sidecar = fs.readFileSync(path.join(root, `${name}.sha256`), "utf8").trim();
    if (sidecar !== `${hash}  ${name}`) throw new Error(`Candidate checksum mismatch: ${platform}`);
    candidate.installers[platform] = { name, sha256: hash };
  }
  return candidate;
}
export function qualify(record, candidate, root, installerRoot = root) {
  const names = candidateNames(candidate);
  if (record.schemaVersion !== 1 || record.commit !== candidate.commit)
    throw new Error("Qualification must identify the exact final candidate commit");
  if (record.version !== candidate.version) throw new Error("Qualification version differs from the candidate version");
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
    const expected = candidate.installers?.[platform];
    if (expected?.name !== names[platform] || !/^[a-f0-9]{64}$/.test(expected?.sha256 || ""))
      throw new Error(`Candidate installer identity is missing or invalid: ${platform}`);
    if (row.installer !== expected.name || row.installerSha256 !== expected.sha256)
      throw new Error(`Qualification installer differs from the candidate: ${platform}`);
    if (!/^[a-f0-9]{64}$/.test(row.installerSha256 || "")) throw new Error(`Installer checksum required: ${platform}`);
    if (!row.installer || path.basename(row.installer) !== row.installer) throw new Error("Expected installer basename");
    if (sha256(fs.readFileSync(path.join(installerRoot, row.installer))) !== row.installerSha256)
      throw new Error(`Installer checksum mismatch: ${platform}`);
  }
  return true;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args[0] === "--candidate" && args.length === 4) {
      verifyTaggedCommit(args[1], args[2]);
      console.log(JSON.stringify(captureCandidate(args[1], args[2], args[3]), null, 2));
    } else {
      const [manifest, candidatePath, flag, installers] = args;
      if (args.length !== 4 || flag !== "--installers") throw new Error("Usage: qualify-release.mjs <evidence.json> <candidate.json> --installers <installer-folder>, or --candidate <commit> <version> <installer-folder>");
      const candidate = JSON.parse(fs.readFileSync(candidatePath, "utf8"));
      verifyTaggedCommit(candidate.commit, candidate.version);
      qualify(JSON.parse(fs.readFileSync(manifest, "utf8")), candidate, path.dirname(path.resolve(manifest)), path.resolve(installers));
      console.log("Recorded candidate qualification passed; review observations before publishing.");
    }
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
