import assert from "node:assert/strict";
import test from "node:test";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { manageDraft, requireDraft, verifyTag, installerNames } from "./draft-release.mjs";

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
