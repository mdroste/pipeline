#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import { packageUrl, updateManifest, updatePackages, updaterPublicKey, verifyUpdateSignature } from "./update-manifest.mjs";

export function verifyTag(ref, annotated, expectedCommit) {
  if (ref.object?.type !== "tag" || annotated.sha !== ref.object.sha)
    throw new Error("Release requires an annotated tag with matching object identity");
  if (annotated.verification?.verified !== true || annotated.verification?.reason !== "valid")
    throw new Error("GitHub has not verified this tag signature");
  if (annotated.object?.type !== "commit" || annotated.object.sha !== expectedCommit)
    throw new Error("Signed tag does not point to the checked-out release commit");
}
export function requireDraft(release) {
  if (!release.isDraft) throw new Error("Published releases are immutable; create a new version");
}
export const installerNames = version => [
  `Pipeline-${version}-macOS-AppleSilicon.dmg`, `Pipeline-${version}-macOS-Intel.dmg`,
  `Pipeline-${version}-Windows.exe`, `Pipeline-${version}-Linux.AppImage`,
];
const digest = file => createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const MANIFEST = "latest.json";
const uploadable = version => new Set([...installerNames(version),
  ...updatePackages(version).flatMap(({ name }) => [name, `${name}.sig`])]);

export function manageDraft(mode, tag, file, gh = args => execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }), updaterKey = updaterPublicKey()) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error("A stable version tag is required");
  const repo = process.env.GITHUB_REPOSITORY;
  const version = tag.slice(1);
  const view = () => JSON.parse(gh(["release", "view", tag, "--json", "isDraft,assets"]));
  // Assets are immutable: a retry may repeat an upload only with identical bytes.
  const publish = (candidate, temp) => {
    const release = view(); requireDraft(release);
    const assetName = path.basename(candidate);
    if (release.assets.some(a => a.name === assetName)) {
      const previous = path.join(temp, "previous"); fs.mkdirSync(previous, { recursive: true });
      gh(["release", "download", tag, "--pattern", assetName, "--dir", previous]);
      if (digest(path.join(previous, assetName)) !== digest(candidate))
        throw new Error(`Existing asset differs: ${assetName}. Use a new version; assets are never overwritten.`);
    } else gh(["release", "upload", tag, candidate]);
  };
  if (mode === "prepare") {
    const ref = JSON.parse(gh(["api", `repos/${repo}/git/ref/tags/${tag}`]));
    if (ref.object?.type !== "tag") throw new Error("Release requires a signed annotated tag");
    const annotated = JSON.parse(gh(["api", `repos/${repo}/git/tags/${ref.object.sha}`]));
    verifyTag(ref, annotated, process.env.GITHUB_SHA);
    let release;
    try { release = view(); } catch {
      // A single coordinator owns creation. Any network/API failure still
      // fails closed if create cannot prove the tag and create the draft.
      gh(["release", "create", tag, "--draft", "--verify-tag", "--title", `Pipeline ${tag}`,
        "--notes", "Candidate installers. macOS builds are signed and notarized; Windows builds use Azure Artifact Signing. Publish only after complete-draft and manual qualification checks."]);
      release = view();
    }
    requireDraft(release);
    return;
  }
  requireDraft(view());
  if (mode === "upload") {
    const name = path.basename(file);
    if (!uploadable(version).has(name)) throw new Error("Unexpected release asset name");
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-release-"));
    try {
      publish(file, temp);
      if (!name.endsWith(".sig")) {
        fs.writeFileSync(path.join(temp, `${name}.sha256`), `${digest(file)}  ${name}\n`);
        publish(path.join(temp, `${name}.sha256`), temp);
      }
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  } else if (mode === "manifest") {
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-manifest-"));
    try {
      const signatures = {};
      for (const { name } of updatePackages(version)) {
        gh(["release", "download", tag, "--pattern", `${name}.sig`, "--dir", temp]);
        signatures[name] = fs.readFileSync(path.join(temp, `${name}.sig`), "utf8");
      }
      const manifest = path.join(temp, MANIFEST);
      fs.writeFileSync(manifest, `${JSON.stringify(updateManifest(version, signatures), null, 2)}\n`);
      publish(manifest, temp);
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  } else if (mode === "verify") {
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-draft-"));
    try {
      for (const name of installerNames(version)) {
        for (const asset of [name, `${name}.sha256`])
          gh(["release", "download", tag, "--pattern", asset, "--dir", temp]);
        const expected = `${digest(path.join(temp, name))}  ${name}\n`;
        if (fs.readFileSync(path.join(temp, `${name}.sha256`), "utf8") !== expected)
          throw new Error(`Checksum mismatch: ${name}`);
      }
      gh(["release", "download", tag, "--pattern", MANIFEST, "--dir", temp]);
      const manifest = JSON.parse(fs.readFileSync(path.join(temp, MANIFEST), "utf8"));
      if (manifest.version !== version) throw new Error(`Manifest is for version ${manifest.version}`);
      for (const { platform, name } of updatePackages(version)) {
        if (!fs.existsSync(path.join(temp, name)))
          gh(["release", "download", tag, "--pattern", name, "--dir", temp]);
        const entry = manifest.platforms?.[platform];
        if (entry?.url !== packageUrl(version, name)) throw new Error(`Manifest has the wrong package for ${platform}`);
        verifyUpdateSignature(fs.readFileSync(path.join(temp, name)), entry.signature, updaterKey, version);
      }
      requireDraft(view());
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  } else throw new Error(`Unknown draft operation: ${mode}`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { manageDraft(...process.argv.slice(2)); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
