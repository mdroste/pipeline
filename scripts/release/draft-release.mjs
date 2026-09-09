#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";

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

export function manageDraft(mode, tag, file, gh = args => execFileSync("gh", args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] })) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error("A stable version tag is required");
  const repo = process.env.GITHUB_REPOSITORY;
  const view = () => JSON.parse(gh(["release", "view", tag, "--json", "isDraft,assets"]));
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
        "--notes", "Candidate installers. macOS is signed and notarized; Windows is unsigned. Publish only after complete-draft and manual qualification checks."]);
      release = view();
    }
    requireDraft(release);
    return;
  }
  requireDraft(view());
  if (mode === "upload") {
    const name = path.basename(file);
    if (!installerNames(tag.slice(1)).includes(name)) throw new Error("Unexpected installer name");
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-release-"));
    try {
      fs.writeFileSync(path.join(temp, `${name}.sha256`), `${digest(file)}  ${name}\n`);
      for (const candidate of [file, path.join(temp, `${name}.sha256`)]) {
        const release = view(); requireDraft(release);
        const assetName = path.basename(candidate);
        if (release.assets.some(a => a.name === assetName)) {
          const previous = path.join(temp, "previous"); fs.mkdirSync(previous, { recursive: true });
          gh(["release", "download", tag, "--pattern", assetName, "--dir", previous]);
          if (digest(path.join(previous, assetName)) !== digest(candidate))
            throw new Error(`Existing asset differs: ${assetName}. Use a new version; assets are never overwritten.`);
        } else gh(["release", "upload", tag, candidate]);
      }
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  } else if (mode === "verify") {
    const temp = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-draft-"));
    try {
      for (const name of installerNames(tag.slice(1))) {
        for (const asset of [name, `${name}.sha256`])
          gh(["release", "download", tag, "--pattern", asset, "--dir", temp]);
        const expected = `${digest(path.join(temp, name))}  ${name}\n`;
        if (fs.readFileSync(path.join(temp, `${name}.sha256`), "utf8") !== expected)
          throw new Error(`Checksum mismatch: ${name}`);
      }
      requireDraft(view());
    } finally { fs.rmSync(temp, { recursive: true, force: true }); }
  } else throw new Error(`Unknown draft operation: ${mode}`);
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { manageDraft(...process.argv.slice(2)); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
