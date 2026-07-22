import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import { collectFiles, verifyFileSet } from "./poppler-provenance.mjs";

test("Poppler file inventory is deterministic and ignores its generated manifest", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-poppler-provenance-"));
  fs.mkdirSync(path.join(dir, "lib"));
  fs.writeFileSync(path.join(dir, "z.txt"), "z");
  fs.writeFileSync(path.join(dir, "lib", "a.txt"), "a");
  fs.writeFileSync(path.join(dir, "PROVENANCE.json"), "generated");
  assert.deepEqual(collectFiles(dir).map((file) => file.path), ["lib/a.txt", "z.txt"]);
});

test("Poppler provenance detects a changed bundled file", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-poppler-tamper-"));
  const file = path.join(dir, "pdftotext");
  fs.writeFileSync(file, "first");
  const expected = collectFiles(dir);
  fs.writeFileSync(file, "changed");
  assert.throws(() => verifyFileSet(dir, expected), /pdftotext/);
});

