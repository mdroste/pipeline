import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { manageDraft, requireDraft, verifyTag, installerNames } from "./draft-release.mjs";
import { updatePackages, updateManifest, updaterPublicKey, verifyUpdateSignature } from "./update-manifest.mjs";

// A minisign keypair and signatures in the format Tauri writes.
function updaterKeys() {
  const { publicKey, privateKey } = generateKeyPairSync("ed25519");
  const id = Buffer.from("0102030405060708", "hex");
  const raw = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  const box = Buffer.concat([Buffer.from("Ed"), id, raw]).toString("base64");
  return {
    publicKey: Buffer.from(`untrusted comment: minisign public key\n${box}\n`).toString("base64"),
    sign(bytes, version, signedId = id) {
      const signature = sign(null, createHash("blake2b512").update(bytes).digest(), privateKey);
      const trusted = `timestamp:1\tfile:package\tversion:${version}`;
      const global = sign(null, Buffer.concat([signature, Buffer.from(trusted)]), privateKey);
      const text = ["untrusted comment: signature from tauri secret key",
        Buffer.concat([Buffer.from("ED"), signedId, signature]).toString("base64"),
        `trusted comment: ${trusted}`, global.toString("base64"), ""].join("\n");
      return Buffer.from(text).toString("base64");
    },
  };
}

// An in-memory draft with the `gh` commands manageDraft uses.
function fakeDraft(assets = new Map()) {
  return args => {
    assert.ok(!args.includes("--clobber"));
    if (args[1] === "view") return JSON.stringify({ isDraft: true, assets: [...assets.keys()].map(name => ({ name })) });
    if (args[1] === "upload") { assets.set(path.basename(args[3]), fs.readFileSync(args[3])); return ""; }
    if (args[1] === "download") {
      const name = args[args.indexOf("--pattern") + 1];
      if (!assets.has(name)) throw new Error(`missing asset ${name}`);
      fs.writeFileSync(path.join(args[args.indexOf("--dir") + 1], name), assets.get(name));
      return "";
    }
    throw new Error("unexpected command");
  };
}

function completeDraft(keys, version = "1.2.3") {
  const assets = new Map();
  for (const name of installerNames(version)) {
    assets.set(name, Buffer.from(name));
    assets.set(`${name}.sha256`, Buffer.from(`${createHash("sha256").update(name).digest("hex")}  ${name}\n`));
  }
  for (const { name } of updatePackages(version)) {
    assets.set(name, Buffer.from(name));
    assets.set(`${name}.sig`, Buffer.from(keys.sign(Buffer.from(name), version)));
  }
  return assets;
}

test("signed tag verification binds ref, tag object and checkout commit", () => {
  const ref = { object: { type: "tag", sha: "tag-object" } };
  const tag = { sha: "tag-object", object: { type: "commit", sha: "commit" }, verification: { verified: true, reason: "valid" } };
  verifyTag(ref, tag, "commit");
  assert.throws(() => verifyTag(ref, tag, "different"), /checked-out/);
  assert.throws(() => verifyTag({ object: { type: "commit" } }, tag, "commit"), /annotated/);
  assert.throws(() => verifyTag(ref, { ...tag, verification: { verified: false } }, "commit"), /verified/);
  assert.throws(() => requireDraft({ isDraft: false }), /immutable/);
});

test("uploads are immutable, retries compare bytes, and public releases reject writes", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "draft-test-"));
  const file = path.join(root, installerNames("1.2.3")[0]);
  fs.writeFileSync(file, "candidate");
  const assets = new Map(); let draft = true; let uploads = 0;
  const gh = args => {
    assert.ok(!args.includes("--clobber"));
    if (args[1] === "view") return JSON.stringify({ isDraft: draft, assets: [...assets.keys()].map(name => ({ name })) });
    if (args[1] === "upload") { uploads++; assets.set(path.basename(args[3]), fs.readFileSync(args[3])); return ""; }
    if (args[1] === "download") {
      const name = args[args.indexOf("--pattern") + 1];
      const dir = args[args.indexOf("--dir") + 1];
      if (!assets.has(name)) throw new Error("missing asset");
      fs.writeFileSync(path.join(dir, name), assets.get(name)); return "";
    }
    throw new Error("unexpected command");
  };
  try {
    manageDraft("upload", "v1.2.3", file, gh);
    assert.equal(uploads, 2);
    manageDraft("upload", "v1.2.3", file, gh);
    assert.equal(uploads, 2);
    assert.throws(() => manageDraft("verify", "v1.2.3", undefined, gh), /missing asset/);
    fs.writeFileSync(file, "changed");
    assert.throws(() => manageDraft("upload", "v1.2.3", file, gh), /differs/);
    draft = false;
    assert.throws(() => manageDraft("upload", "v1.2.3", file, gh), /immutable/);
    assert.equal(uploads, 2);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("the update manifest joins each package with its signature and the draft verifies it", () => {
  const keys = updaterKeys();
  const assets = completeDraft(keys);
  const gh = fakeDraft(assets);
  assert.throws(() => manageDraft("verify", "v1.2.3", undefined, gh, keys.publicKey), /missing asset latest\.json/);

  manageDraft("manifest", "v1.2.3", undefined, gh, keys.publicKey);
  const manifest = JSON.parse(assets.get("latest.json"));
  assert.equal(manifest.version, "1.2.3");
  assert.deepEqual(Object.keys(manifest.platforms).sort(),
    ["darwin-aarch64", "darwin-x86_64", "linux-x86_64", "windows-x86_64"]);
  assert.equal(manifest.platforms["darwin-aarch64"].url,
    "https://github.com/mdroste/pipeline/releases/download/v1.2.3/Pipeline-1.2.3-macOS-AppleSilicon.app.tar.gz");
  manageDraft("verify", "v1.2.3", undefined, gh, keys.publicKey);

  // A retried job writes the same manifest, so the immutable upload accepts it.
  manageDraft("manifest", "v1.2.3", undefined, gh, keys.publicKey);
  assets.set("Pipeline-1.2.3-Linux.AppImage.sig", Buffer.from(keys.sign(Buffer.from("changed"), "1.2.3")));
  assert.throws(() => manageDraft("manifest", "v1.2.3", undefined, gh, keys.publicKey), /Existing asset differs/);
});

test("a draft with a package the app would reject cannot pass", () => {
  const keys = updaterKeys();
  const assets = completeDraft(keys);
  const gh = fakeDraft(assets);
  manageDraft("manifest", "v1.2.3", undefined, gh, keys.publicKey);
  const name = "Pipeline-1.2.3-macOS-Intel.app.tar.gz";
  assets.set(name, Buffer.from("tampered"));
  assert.throws(() => manageDraft("verify", "v1.2.3", undefined, gh, keys.publicKey), /doesn't match its signature/);
  assets.set(name, Buffer.from(name));
  assert.throws(() => manageDraft("verify", "v1.2.3", undefined, gh, updaterKeys().publicKey), /doesn't match its signature/);
});

test("uploads accept update packages and signatures but nothing else", () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "draft-test-"));
  const assets = new Map();
  const gh = fakeDraft(assets);
  const write = name => { const file = path.join(root, name); fs.writeFileSync(file, name); return file; };
  try {
    manageDraft("upload", "v1.2.3", write("Pipeline-1.2.3-macOS-Intel.app.tar.gz"), gh, "unused");
    manageDraft("upload", "v1.2.3", write("Pipeline-1.2.3-macOS-Intel.app.tar.gz.sig"), gh, "unused");
    assert.deepEqual([...assets.keys()].sort(), ["Pipeline-1.2.3-macOS-Intel.app.tar.gz",
      "Pipeline-1.2.3-macOS-Intel.app.tar.gz.sha256", "Pipeline-1.2.3-macOS-Intel.app.tar.gz.sig"]);
    assert.throws(() => manageDraft("upload", "v1.2.3", write("latest.json"), gh, "unused"), /Unexpected/);
    assert.throws(() => manageDraft("upload", "v1.2.3", write("Pipeline-1.2.2-Windows.exe.sig"), gh, "unused"), /Unexpected/);
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

test("update signatures bind the key, the bytes, and the version", () => {
  const keys = updaterKeys();
  const bytes = Buffer.from("package");
  verifyUpdateSignature(bytes, keys.sign(bytes, "1.2.3"), keys.publicKey, "1.2.3");
  assert.throws(() => verifyUpdateSignature(bytes, keys.sign(bytes, "1.2.2"), keys.publicKey, "1.2.3"), /wasn't signed for version/);
  assert.throws(() => verifyUpdateSignature(bytes, keys.sign(bytes, "1.2.3", Buffer.alloc(8)), keys.publicKey, "1.2.3"), /doesn't trust/);
  const altered = Buffer.from(keys.sign(bytes, "1.2.3"), "base64").toString().replace("version:1.2.3", "version:1.2.4");
  assert.throws(() => verifyUpdateSignature(bytes, Buffer.from(altered).toString("base64"), keys.publicKey, "1.2.4"), /altered/);
  assert.throws(() => verifyUpdateSignature(bytes, "not a signature", keys.publicKey, "1.2.3"), /Unreadable/);
});

test("the manifest needs a signature for every platform", () => {
  assert.throws(() => updateManifest("1.2.3", {}), /Missing updater signature/);
});

test("the app trusts a configured updater key that must carry a signed version", () => {
  assert.match(Buffer.from(updaterPublicKey(), "base64").toString(), /^untrusted comment: minisign public key/);
  const config = JSON.parse(fs.readFileSync(new URL("../../gui/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(config.plugins.updater.requireSignedVersion, true);
  // Contributors have no signing key, so only release builds create packages.
  assert.equal(config.bundle.createUpdaterArtifacts, undefined);
});
