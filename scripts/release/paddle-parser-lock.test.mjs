import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const RESOURCE_DIR = path.join(
  ROOT,
  "gui",
  "src-tauri",
  "resources",
  "paddle-parser",
);
const read = (file) => fs.readFileSync(path.join(RESOURCE_DIR, file), "utf8").replaceAll("\r\n", "\n");
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");

test("managed Paddle parser inputs are complete checksum locks", () => {
  const runtime = JSON.parse(read("runtime-lock.json"));
  assert.equal(runtime.schemaVersion, 1);
  assert.equal(runtime.python.version, "3.12.13");
  assert.equal(runtime.uv.version, "0.11.26");
  assert.deepEqual(runtime.packages, {
    paddleocr: "3.7.0",
    paddlepaddle: "3.2.1",
  });
  assert.match(runtime.layoutModel.url, /^https:\/\/paddle-model-ecology\.bj\.bcebos\.com\//);
  assert.match(runtime.layoutModel.sha256, /^[a-f0-9]{64}$/);

  const expectedPlatforms = [
    "linux-aarch64",
    "linux-x86_64",
    "macos-arm64",
    "windows-x86_64",
  ];
  assert.deepEqual(Object.keys(runtime.python.artifacts).sort(), expectedPlatforms);
  assert.deepEqual(Object.keys(runtime.uv.artifacts).sort(), expectedPlatforms);
  const lockedPackages = new Set();

  for (const platform of expectedPlatforms) {
    const artifact = runtime.python.artifacts[platform];
    assert.match(artifact.sha256, /^[a-f0-9]{64}$/);
    assert.match(artifact.lockSha256, /^[a-f0-9]{64}$/);
    const lock = read(artifact.lock);
    assert.equal(sha256(lock), artifact.lockSha256, `${platform} lock checksum drifted`);
    assert.match(lock, /^lock-version = "1\.0"$/m);
    assert.match(lock, /^requires-python = ">=3\.12\.13"$/m);
    assert.match(lock, /name = "paddleocr"\nversion = "3\.7\.0"/);
    assert.match(lock, /name = "paddlepaddle"\nversion = "3\.2\.1"/);
    assert.doesNotMatch(lock, /^sdist\s*=/m, `${platform} permits a source build`);
    const packages = lock.split("[[packages]]").slice(1);
    assert.ok(packages.length >= 90, `${platform} lock is unexpectedly incomplete`);
    for (const entry of packages) {
      const name = entry.match(/^\s*name = "([^"]+)"$/m)?.[1];
      const version = entry.match(/^version = "([^"]+)"$/m)?.[1];
      assert.ok(name && version);
      lockedPackages.add(`${name.toLowerCase().replace(/[-_.]+/g, "-")}@${version}`);
      assert.match(entry, /^wheels = /m);
      for (const [, url] of entry.matchAll(/url = "([^"]+)"/g)) {
        assert.match(url, /^https:\/\/files\.pythonhosted\.org\//);
      }
      for (const [, digest] of entry.matchAll(/sha256 = "([^"]+)"/g)) {
        assert.match(digest, /^[a-f0-9]{64}$/);
      }
      assert.ok(entry.includes("sha256 = \""), "every package must carry a wheel hash");
    }
    assert.match(runtime.uv.artifacts[platform].sha256, /^[a-f0-9]{64}$/);
  }
  const licenses = JSON.parse(read("python-licenses.json"));
  const licensedPackages = new Set(licenses.packages.map((item) => {
    assert.ok(item.declaredLicense, `${item.name} has no declared license evidence`);
    return `${item.name.toLowerCase().replace(/[-_.]+/g, "-")}@${item.version}`;
  }));
  assert.deepEqual(licensedPackages, lockedPackages);
});

test("parser lock metadata is bound into runtime and release qualification", () => {
  const requirements = read("requirements.in")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
  assert.deepEqual(requirements, [
    "paddleocr[doc-parser]==3.7.0",
    "paddlepaddle==3.2.1",
  ]);

  const engines = fs.readFileSync(
    path.join(ROOT, "gui", "src-tauri", "src", "engines.rs"),
    "utf8",
  );
  assert.match(engines, /include_str!\("\.\.\/resources\/paddle-parser\/runtime-lock\.json"\)/);
  assert.match(engines, /"runtime_lock_sha256": runtime_lock_sha256/);
  assert.match(engines, /"layout_ready": true/);

  const release = fs.readFileSync(path.join(ROOT, ".github", "workflows", "release.yml"), "utf8");
  assert.match(release, /needs: \[quality, paddle-parser-qualification\]/);
  const qualification = fs.readFileSync(
    path.join(ROOT, ".github", "workflows", "paddle-parser-qualification.yml"),
    "utf8",
  );
  for (const runner of ["macos-15", "windows-2022", "ubuntu-22.04", "ubuntu-24.04-arm"]) {
    assert.ok(qualification.includes(`os: ${runner}`));
  }
  assert.ok(qualification.includes("qualify-paddle-parser.py"));
});
