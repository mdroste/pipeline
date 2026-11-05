// The in-app updater's release assets. A release carries one signed package per
// platform and a `latest.json` manifest naming them. The manifest isn't signed:
// the app verifies each package against the key in tauri.conf.json, and these
// helpers apply the same check before a draft can be published.
import { createHash, createPublicKey, verify } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPOSITORY = "mdroste/pipeline";
const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");
const TAURI_CONFIG = path.resolve(path.dirname(fileURLToPath(import.meta.url)),
  "../../gui/src-tauri/tauri.conf.json");

// Windows and Linux install the installer itself. The names must match
// PACKAGE_SUFFIX in gui/src-tauri/src/updates/github.rs.
export const updatePackages = version => [
  { platform: "darwin-aarch64", name: `Pipeline-${version}-macOS-AppleSilicon.app.tar.gz` },
  { platform: "darwin-x86_64", name: `Pipeline-${version}-macOS-Intel.app.tar.gz` },
  { platform: "windows-x86_64", name: `Pipeline-${version}-Windows.exe` },
  { platform: "linux-x86_64", name: `Pipeline-${version}-Linux.AppImage` },
];

export const packageUrl = (version, name) =>
  `https://github.com/${REPOSITORY}/releases/download/v${version}/${name}`;

export const updaterPublicKey = () =>
  JSON.parse(fs.readFileSync(TAURI_CONFIG, "utf8")).plugins.updater.pubkey;

/** `signatures` maps each package name to the contents of its `.sig` file. */
export function updateManifest(version, signatures) {
  const platforms = {};
  for (const { platform, name } of updatePackages(version)) {
    const signature = signatures[name]?.trim();
    if (!signature) throw new Error(`Missing updater signature: ${name}`);
    platforms[platform] = { signature, url: packageUrl(version, name) };
  }
  return { version, platforms };
}

// Tauri wraps minisign's text formats in base64.
const lines = encoded => Buffer.from(encoded, "base64").toString("utf8").split("\n");

/** Applies the checks the app makes, so a bad signature can't be published. */
export function verifyUpdateSignature(bytes, signature, publicKey, version) {
  const keyBox = Buffer.from(lines(publicKey)[1] ?? "", "base64");
  if (keyBox.length !== 42 || keyBox.subarray(0, 2).toString() !== "Ed")
    throw new Error("Unreadable updater public key");
  const key = createPublicKey({
    key: Buffer.concat([ED25519_SPKI_PREFIX, keyBox.subarray(10)]), format: "der", type: "spki",
  });
  const [, encodedSignature, comment, encodedGlobal] = lines(signature);
  const box = Buffer.from(encodedSignature ?? "", "base64");
  if (box.length !== 74 || box.subarray(0, 2).toString() !== "ED")
    throw new Error("Unreadable updater signature");
  if (!box.subarray(2, 10).equals(keyBox.subarray(2, 10)))
    throw new Error("Package was signed with a key the app doesn't trust");
  const signed = box.subarray(10);
  if (!verify(null, createHash("blake2b512").update(bytes).digest(), key, signed))
    throw new Error("Package doesn't match its signature");
  const trusted = (comment ?? "").replace(/^trusted comment: /, "");
  const coverage = Buffer.concat([signed, Buffer.from(trusted)]);
  if (!verify(null, coverage, key, Buffer.from(encodedGlobal ?? "", "base64")))
    throw new Error("Signature comment was altered");
  if (!trusted.split("\t").includes(`version:${version}`))
    throw new Error(`Package wasn't signed for version ${version}`);
}
