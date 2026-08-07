import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import assert from "node:assert/strict";

import {
  collectFiles,
  sha256File,
  validateNativePackagePolicy,
  verifyWindowsCondaPayloads,
  verifyFileSet,
} from "./poppler-provenance.mjs";

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

test("Linux native inventory requires versioned packages and per-package copyright files", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-poppler-license-"));
  fs.mkdirSync(path.join(dir, "licenses", "debian"), { recursive: true });
  fs.mkdirSync(path.join(dir, "share", "poppler"), { recursive: true });
  fs.writeFileSync(path.join(dir, "licenses", "debian", "libpoppler118.copyright"), "terms");
  const inventory = {
    manager: "dpkg",
    packages: [{ name: "libpoppler118:amd64", version: "22.02.0-2ubuntu0.13" }],
  };
  assert.doesNotThrow(() => validateNativePackagePolicy(dir, "linux", inventory));
  assert.throws(
    () => validateNativePackagePolicy(dir, "linux", {
      ...inventory,
      packages: [{ name: "missing:amd64", version: "1" }],
    }),
    /copyright file/,
  );
});

test("Windows native inventory requires versioned conda attribution for every shipped PE file", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-poppler-license-"));
  fs.mkdirSync(path.join(dir, "licenses", "poppler"), { recursive: true });
  fs.mkdirSync(path.join(dir, "share", "poppler"), { recursive: true });
  fs.writeFileSync(path.join(dir, "licenses", "poppler", "COPYING"), "terms");
  fs.writeFileSync(path.join(dir, "pdftotext.exe"), "exact executable");
  const packageRoot = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-conda-payload-"));
  const packageKey = "poppler-25.12.0-hb0e4504_0";
  fs.mkdirSync(path.join(packageRoot, packageKey, "Library", "bin"), { recursive: true });
  fs.writeFileSync(path.join(packageRoot, `${packageKey}.conda`), "exact package archive");
  fs.writeFileSync(
    path.join(packageRoot, packageKey, "Library", "bin", "pdftotext.exe"),
    "exact executable",
  );
  const inventory = {
    manager: "conda",
    channel: "conda-forge",
    subdir: "win-64",
    packages: [
      {
        name: "poppler",
        version: "25.12.0",
        build: "hb0e4504_0",
        license: "GPL-2.0-only",
        sourceUrl: "https://conda.anaconda.org/conda-forge/win-64/poppler-25.12.0-hb0e4504_0.conda",
        archiveSha256: sha256File(path.join(packageRoot, `${packageKey}.conda`)),
        files: [{
          path: "pdftotext.exe",
          sha256: sha256File(path.join(dir, "pdftotext.exe")),
        }],
      },
    ],
  };
  assert.doesNotThrow(() => validateNativePackagePolicy(dir, "windows", inventory));
  assert.doesNotThrow(
    () => verifyWindowsCondaPayloads(dir, packageRoot, inventory),
  );
  fs.writeFileSync(
    path.join(packageRoot, packageKey, "Library", "bin", "pdftotext.exe"),
    "changed source payload",
  );
  assert.throws(
    () => verifyWindowsCondaPayloads(dir, packageRoot, inventory),
    /not present in pinned conda payload/,
  );
  assert.throws(
    () => validateNativePackagePolicy(dir, "windows", {
      ...inventory,
      packages: [{ ...inventory.packages[0], version: undefined }],
    }),
    /has no version/,
  );
  fs.writeFileSync(path.join(dir, "unattributed.dll"), "unowned dependency");
  assert.throws(
    () => validateNativePackagePolicy(dir, "windows", inventory),
    /unattributed\.dll/,
  );
});
