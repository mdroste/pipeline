#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
const filename = "BUNDLE_INVENTORY.json";
export function inventoryFiles(root) {
  const files = [];
  function walk(folder) {
    for (const name of fs.readdirSync(folder).sort()) {
      const absolute = path.join(folder, name);
      const relative = path.relative(root, absolute).split(path.sep).join("/");
      if (relative === filename) continue;
      const stat = fs.lstatSync(absolute);
      if (stat.isDirectory()) walk(absolute);
      else if (stat.isFile()) files.push({ path: relative, bytes: stat.size, sha256: crypto.createHash("sha256").update(fs.readFileSync(absolute)).digest("hex") });
      else throw new Error(`Unsupported bundled file: ${relative}`);
    }
  }
  walk(root);
  return files;
}
export function verifyInventory(root) {
  const recorded = JSON.parse(fs.readFileSync(path.join(root, filename), "utf8"));
  if (recorded.schemaVersion !== 1 || JSON.stringify(recorded.files) !== JSON.stringify(inventoryFiles(root)))
    throw new Error("Bundled Poppler files changed or are missing; rebuild the resource bundle");
  return recorded;
}
export function writeInventory(root, platform = process.platform) {
  const binary = path.join(root, platform === "windows" || platform === "win32" ? "pdftotext.exe" : "pdftotext");
  const child = spawnSync(binary, ["-v"], { encoding: "utf8", timeout: 15000, maxBuffer: 65536 });
  if (child.error || child.status !== 0) throw new Error(`Cannot identify bundled Poppler: ${child.error || child.stderr}`);
  return writeObservedInventory(root, platform, `${child.stdout}\n${child.stderr}`);
}
function writeObservedInventory(root, platform, version) {
  const result = { schemaVersion: 1, platform, versionOutput: version.trim(), note: "Observed bundled files and hashes; not a complete dependency SBOM or corresponding-source offer.", files: inventoryFiles(root) };
  fs.writeFileSync(path.join(root, filename), JSON.stringify(result, null, 2) + "\n");
  return result;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../gui/src-tauri/resources/poppler");
  if (process.argv[2] === "--verify") verifyInventory(root);
  else writeInventory(root, process.argv[2]);
}
