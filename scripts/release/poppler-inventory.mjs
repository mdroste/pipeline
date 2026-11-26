#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
const filename = "BUNDLE_INVENTORY.json";
export function popplerBinary(root, tool, windows = process.platform === "win32") {
  const name = tool + (windows ? ".exe" : "");
  const directories = windows ? ["Library/bin", "bin", ""] : [""];
  return directories.map(directory => path.join(root, directory, name)).find(file => fs.existsSync(file)) ?? path.join(root, name);
}
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
  const binary = popplerBinary(root, "pdftotext", platform === "windows" || platform === "win32");
  const child = spawnSync(binary, ["-v"], { encoding: "utf8", timeout: 15000, maxBuffer: 65536 });
  if (child.error || child.status !== 0) throw new Error(`Cannot identify bundled Poppler: ${child.error || child.stderr}`);
  return writeObservedInventory(root, platform, `${child.stdout}\n${child.stderr}`);
}
function writeObservedInventory(root, platform, version) {
  const result = { schemaVersion: 1, platform, versionOutput: version.trim(), note: "Observed bundled files and hashes; not a complete dependency SBOM or corresponding-source offer.", files: inventoryFiles(root) };
  const inventory = path.join(root, filename), contents = JSON.stringify(result, null, 2) + "\n";
  // Tauri's build watches this file, so an unchanged rewrite would recompile the app.
  if (!fs.existsSync(inventory) || fs.readFileSync(inventory, "utf8") !== contents) fs.writeFileSync(inventory, contents);
  return result;
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../gui/src-tauri/resources/poppler");
  // `--verify [folder]` checks an installed copy too; with no folder, the bundle.
  if (process.argv[2] === "--verify") verifyInventory(process.argv[3] ? path.resolve(process.argv[3]) : root);
  else writeInventory(root, process.argv[2]);
}
