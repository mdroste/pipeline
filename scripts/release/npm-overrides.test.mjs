import assert from "node:assert/strict";
import fs from "node:fs";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import test from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const GUI = path.join(ROOT, "gui");
const requireFromGui = createRequire(path.join(GUI, "package.json"));

test("security overrides preserve WebdriverIO's legacy glob consumers", async () => {
  const recursiveReaddir = requireFromGui("recursive-readdir");
  const { FileList } = requireFromGui("filelist");
  const fixture = await mkdtemp(path.join(os.tmpdir(), "pipeline-npm-overrides-"));

  try {
    const nested = path.join(fixture, "nested");
    await mkdir(nested);
    await writeFile(path.join(nested, "keep.txt"), "keep");
    await writeFile(path.join(nested, "ignore.skip"), "ignore");

    const recursivelyRead = await recursiveReaddir(fixture, ["**/*.skip"]);
    assert.deepEqual(recursivelyRead.map((entry) => path.basename(entry)), ["keep.txt"]);

    // filelist pushes the pattern through path.normalize() before minimatch,
    // which rewrites `/` as `\` on Windows; minimatch then reads `\` as an
    // escape and matches nothing. That is true of filelist's own declared
    // minimatch ^5.0.1 as well as the 10.2.6 the override pins, so the option
    // below is how any caller uses filelist with an absolute path. It is a
    // no-op on POSIX, where the pattern holds no backslashes.
    const files = new FileList();
    files.include(
      { windowsPathsNoEscape: true },
      path.join(fixture, "**", "*.txt").replaceAll(path.sep, "/"),
    );
    assert.deepEqual(files.toArray().map((entry) => path.basename(entry)), ["keep.txt"]);
  } finally {
    await rm(fixture, { recursive: true, force: true });
  }
});

test("overridden Mocha and diff execute together", async () => {
  const Mocha = requireFromGui("mocha");
  const diff = requireFromGui("diff");
  const mocha = new Mocha({ reporter: class SilentReporter {} });
  const suite = Mocha.Suite.create(mocha.suite, "override smoke");

  suite.addTest(new Mocha.Test("runs", () => {
    const changes = diff.diffChars("safe", "safer");
    assert.ok(changes.some((change) => change.added));
  }));

  const failures = await new Promise((resolve) => mocha.run(resolve));
  assert.equal(failures, 0);
});

test("WebdriverIO CLI and project configuration remain loadable", async () => {
  await import(pathToFileURL(requireFromGui.resolve("@wdio/cli")).href);
  const { config } = await import(pathToFileURL(path.join(GUI, "wdio.conf.mjs")).href);

  assert.equal(config.framework, "mocha");
  assert.deepEqual(config.specs, ["./e2e/**/*.spec.mjs"]);
  assert.ok(config.services.flat(Infinity).includes("@wdio/tauri-service"));
});

test("audited override versions stay locked", () => {
  const lock = JSON.parse(fs.readFileSync(path.join(GUI, "package-lock.json"), "utf8"));
  const expected = {
    "node_modules/brace-expansion": "5.0.9",
    "node_modules/diff": "8.0.4",
    "node_modules/esbuild": "0.28.1",
    "node_modules/ip-address": "10.4.0",
    "node_modules/js-yaml": "4.3.1",
    "node_modules/minimatch": "10.2.6",
    "node_modules/mocha": "11.7.6",
    "node_modules/nanoid": "3.3.18",
    "node_modules/serialize-javascript": "7.0.7",
    "node_modules/undici": "7.29.0",
    "node_modules/webdriver/node_modules/undici": "6.28.0",
    "node_modules/@wdio/tauri-service/node_modules/undici": "6.28.0",
  };

  for (const [packagePath, version] of Object.entries(expected)) {
    assert.equal(lock.packages[packagePath]?.version, version, `${packagePath} must remain audited`);
  }
});
