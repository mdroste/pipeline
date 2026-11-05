import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  cargoProductionPackages,
  collectLibraries,
  npmProductionPackages,
  readLicenseTexts,
  renderLibraryNotice,
  unlicensedPackages,
} from "./library-licenses.mjs";

const ROOT = fileURLToPath(new URL("../../", import.meta.url));

function withFolder(run) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-licenses-"));
  try {
    return run(folder);
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
}

const crate = (name, source = "registry") => ({
  id: name,
  name,
  version: "1.0.0",
  source,
  license: "MIT",
  authors: [],
  manifest_path: `/registry/${name}/Cargo.toml`,
});
const normal = (pkg) => ({ pkg, dep_kinds: [{ kind: null, target: null }] });
const devOnly = (pkg) => ({ pkg, dep_kinds: [{ kind: "dev", target: null }] });

test("lists crates reached through normal dependencies, for any target", () => {
  const packages = cargoProductionPackages({
    packages: [crate("app", null), crate("direct"), crate("nested"), crate("windows-only"), crate("test-tool")],
    resolve: {
      root: "app",
      nodes: [
        { id: "app", deps: [normal("direct"), devOnly("test-tool"), { pkg: "windows-only", dep_kinds: [{ kind: null, target: "cfg(windows)" }] }] },
        { id: "direct", deps: [normal("nested")] },
        { id: "nested", deps: [] },
        { id: "windows-only", deps: [] },
        { id: "test-tool", deps: [] },
      ],
    },
  });
  assert.deepEqual(packages.map((item) => item.name).sort(), ["direct", "nested", "windows-only"]);
});

test("lists production npm packages once, without development or platform add-ons", () => {
  const packages = npmProductionPackages({
    packages: {
      "": { name: "root", version: "1.0.0" },
      "node_modules/react": { version: "19.0.0", license: "MIT" },
      "node_modules/a/node_modules/react": { version: "19.0.0", license: "MIT" },
      "node_modules/vite": { version: "8.0.0", dev: true },
      "node_modules/fsevents": { version: "2.0.0", devOptional: true },
      "node_modules/@napi-rs/canvas-darwin-arm64": { version: "1.0.0", optional: true },
    },
  }, "/gui");
  assert.deepEqual(packages.map((item) => `${item.name}@${item.version}`), ["react@19.0.0"]);
});

test("reads license files but not source files that share the name", () => {
  withFolder((folder) => {
    fs.writeFileSync(path.join(folder, "LICENSE-MIT"), "\uFEFFMIT text\r\nline two\r\n");
    fs.writeFileSync(path.join(folder, "COPYING.txt"), "copying text");
    fs.writeFileSync(path.join(folder, "license.js"), "module.exports = 1");
    fs.writeFileSync(path.join(folder, "README.md"), "read me");
    assert.deepEqual(readLicenseTexts(folder).sort(), ["MIT text\nline two", "copying text"].sort());
  });
});

test("reads the licenses pdf.js keeps beside its fonts and decoders", () => {
  withFolder((folder) => {
    fs.mkdirSync(path.join(folder, "wasm"));
    fs.writeFileSync(path.join(folder, "wasm", "LICENSE_OPENJPEG"), "openjpeg text");
    assert.deepEqual(readLicenseTexts(folder), []);
    assert.deepEqual(readLicenseTexts(folder, { subfolders: ["wasm"] }), ["openjpeg text"]);
  });
});

test("the notice is byte-identical whatever order or location its inputs have", () => {
  const library = (name, directory, texts) => ({
    ecosystem: "crate", name, version: "1.0.0", license: "MIT", authors: ["A"],
    repository: "https://example.test", directory, texts,
  });
  const first = [library("b", "/home/one/b", ["shared"]), library("a", "/home/one/a", ["shared"]), library("c", "/home/one/c", [])];
  const second = [library("c", "D:/two/c", []), library("a", "D:/two/a", ["shared"]), library("b", "D:/two/b", ["shared"])];
  const notice = renderLibraryNotice("1.2.3", first);
  assert.equal(notice, renderLibraryNotice("1.2.3", second));
  assert.doesNotMatch(notice, /home|D:/);
  assert.equal([...notice.matchAll(/^shared$/gm)].length, 1, "shared text appears once");
  assert.match(notice, /Used by: crate a 1\.0\.0, crate b 1\.0\.0/);
  assert.match(notice, /Published without a license file[\s\S]*crate c 1\.0\.0: MIT\n {2}Authors: A/);
});

test("an official build refuses packages with no license evidence", () => {
  withFolder((folder) => {
    const packages = [
      { ecosystem: "npm", name: "declared", version: "1.0.0", license: "MIT", directory: folder, texts: [] },
      { ecosystem: "npm", name: "text", version: "1.0.0", license: null, directory: folder, texts: ["text"] },
      { ecosystem: "npm", name: "unknown", version: "1.0.0", license: null, directory: folder, texts: [] },
      { ecosystem: "npm", name: "absent", version: "1.0.0", license: "MIT", directory: path.join(folder, "absent"), texts: [] },
    ];
    assert.deepEqual(unlicensedPackages(packages), ["npm absent 1.0.0", "npm unknown 1.0.0"]);
  });
});

test("the notice for this repository names every production dependency", (t) => {
  const metadata = spawnSync("cargo", ["metadata", "--locked", "--offline", "--format-version", "1",
    "--manifest-path", path.join(ROOT, "gui/src-tauri/Cargo.toml")], { encoding: "utf8", maxBuffer: 256 * 1024 * 1024 });
  if (metadata.status !== 0 || !fs.existsSync(path.join(ROOT, "gui/node_modules/react"))) {
    t.skip("needs fetched crates and npm packages");
    return;
  }
  const libraries = collectLibraries(ROOT);
  const notice = renderLibraryNotice("0.0.0", libraries);
  assert.deepEqual(unlicensedPackages(libraries), []);

  const manifest = JSON.parse(fs.readFileSync(path.join(ROOT, "gui/package.json"), "utf8"));
  for (const name of Object.keys(manifest.dependencies)) {
    assert.match(notice, new RegExp(`^npm ${name.replaceAll(".", "\\.")} `, "m"), name);
  }
  const cargo = fs.readFileSync(path.join(ROOT, "gui/src-tauri/Cargo.toml"), "utf8");
  const dependencies = cargo.split("[dependencies]")[1].split(/\n\[/)[0];
  for (const [, name, rest] of dependencies.matchAll(/^([A-Za-z0-9_-]+)\s*=\s*(.*)$/gm)) {
    if (rest.includes("optional = true") || rest.includes("path =")) continue;
    assert.match(notice, new RegExp(`^crate ${name} `, "m"), name);
  }
  // These decoders and fonts ship in the app, and their licenses aren't in the
  // package's top-level LICENSE.
  for (const text of ["JBIG2", "OPENJPEG"].map((name) => path.join(ROOT, `gui/node_modules/pdfjs-dist/wasm/LICENSE_${name}`))) {
    assert.ok(notice.includes(fs.readFileSync(text, "utf8").replaceAll("\r\n", "\n").trim()));
  }
});
