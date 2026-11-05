import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const tauriRoot = path.join(root, "gui", "src-tauri");
const config = JSON.parse(fs.readFileSync(path.join(tauriRoot, "tauri.conf.json"), "utf8"));
const artScript = fs.readFileSync(path.join(root, "scripts/release/render-installer-art.py"), "utf8");
const pointOf = (name) => {
  const [, x, y] = artScript.match(new RegExp(`^${name} = \\((\\d+), (\\d+)\\)$`, "m"));
  return { x: Number(x), y: Number(y) };
};

test("the disk image layout matches the picture drawn for it", () => {
  const { dmg } = config.bundle.macOS;
  const window = pointOf("DMG_WINDOW");
  assert.deepEqual(dmg.windowSize, { width: window.x, height: window.y });
  assert.deepEqual(dmg.appPosition, pointOf("APP_ICON"));
  assert.deepEqual(dmg.applicationFolderPosition, pointOf("APPLICATIONS_ICON"));
  assert.ok(fs.existsSync(path.join(tauriRoot, dmg.background)));
  // verify-macos-dmg.sh looks for the picture under this name.
  assert.equal(path.basename(dmg.background), "dmg-background.tiff");
});

test("the release job keeps the Finder layout step and checks its result", () => {
  const release = fs.readFileSync(path.join(root, ".github/workflows/release.yml"), "utf8");
  const macBuild = release.split("name: Build signed and notarized macOS package")[1]?.split("- name:")[0] ?? "";
  assert.match(macBuild, /TAURI_BUNDLER_DMG_IGNORE_CI: "true"/);
  assert.ok(release.indexOf("verify-macos-dmg.sh") > release.indexOf("name: Build signed and notarized macOS package"));
  assert.ok(release.indexOf("verify-macos-dmg.sh") < release.indexOf("name: Upload immutable installer to draft"));
});

test("opening an installer does not stop at a license agreement", () => {
  // Pipeline's license is permissive, and the app carries it among its notices.
  assert.equal(config.bundle.licenseFile, undefined);
  assert.ok(config.bundle.resources.includes("resources/notices/PIPELINE_LICENSE.txt"));
});

test("Windows installer bitmaps are 24-bit, uncompressed, and in NSIS proportions", () => {
  const { nsis } = config.bundle.windows;
  for (const [image, nominal] of [[nsis.sidebarImage, [164, 314]], [nsis.headerImage, [150, 57]]]) {
    const bytes = fs.readFileSync(path.join(tauriRoot, image));
    assert.equal(bytes.toString("latin1", 0, 2), "BM", image);
    assert.equal(bytes.readUInt16LE(28), 24, image);
    assert.equal(bytes.readUInt32LE(30), 0, image);
    assert.deepEqual([bytes.readInt32LE(18), bytes.readInt32LE(22)], nominal.map((side) => side * 2), image);
  }
  assert.equal(nsis.uninstallerHeaderImage, nsis.headerImage);
});

test("the NSIS hooks stay readable in any build machine's code page", () => {
  const hooks = fs.readFileSync(path.join(tauriRoot, config.bundle.windows.nsis.installerHooks));
  assert.ok(hooks.every((byte) => byte < 0x80));
});
