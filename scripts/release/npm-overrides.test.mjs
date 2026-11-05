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

test("patched transitive dependencies stay locked without vulnerable ZIP extraction", () => {
  const lock = JSON.parse(fs.readFileSync(path.join(GUI, "package-lock.json"), "utf8"));
  const versions = (name) => Object.entries(lock.packages)
    .filter(([location]) => location.endsWith(`node_modules/${name}`))
    .map(([, entry]) => entry.version);
  for (const [name, version] of Object.entries({
    vitest: "4.1.12",
    "@vitest/mocker": "4.1.12",
    "@puppeteer/browsers": "3.2.6",
    "serialize-javascript": "7.1.4",
  })) {
    assert.ok(versions(name).length > 0, `${name} must be installed`);
    assert.ok(versions(name).every((actual) => actual === version), `${name} must remain patched`);
  }
  assert.deepEqual(versions("extract-zip"), [], "browser downloads must not use extract-zip");
  assert.deepEqual(versions("katex"), [lock.packages["node_modules/katex"].version]);
  assert.match(versions("katex")[0], /^0\.18\.(?:[2-9]|[1-9]\d+)$/);
  const undici = { 6: "6.29.1", 7: "7.30.1", 8: "8.12.3" };
  assert.ok(versions("undici").length > 0);
  for (const version of versions("undici")) {
    assert.equal(version, undici[version.split(".")[0]], "every undici copy must be patched");
  }
});
