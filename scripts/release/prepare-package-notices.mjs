#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { LIBRARY_NOTICE, libraryNotice } from "./library-licenses.mjs";
import { writeInventory } from "./poppler-inventory.mjs";
import { validateReleaseIdentity } from "./validate-release-identity.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = path.resolve(path.dirname(scriptPath), "../..");
const version = validateReleaseIdentity({ rootDir: root });
const official = process.env.PIPELINE_OFFICIAL_RELEASE === "1";
const output = path.join(root, "gui", "src-tauri", "resources", "notices");

// Tauri's build watches these files, so rewriting one unchanged would recompile
// the app on every `npm run tauri dev`.
function writeChanged(file, contents) {
  if (!fs.existsSync(file) || fs.readFileSync(file, "utf8") !== contents) fs.writeFileSync(file, contents);
}

fs.mkdirSync(output, { recursive: true });
const poppler = path.join(root, "gui/src-tauri/resources/poppler");
if (fs.existsSync(path.join(poppler, process.platform === "win32" ? "pdftotext.exe" : "pdftotext")))
  writeInventory(poppler, process.env.PIPELINE_BUNDLE_PLATFORM || process.platform);
else if (official) throw new Error("Official releases require a bundled Poppler inventory");
writeChanged(path.join(output, "PIPELINE_LICENSE.txt"), fs.readFileSync(path.join(root, "LICENSE"), "utf8"));
writeChanged(
  path.join(output, "THIRD_PARTY_LICENSES.md"),
  fs.readFileSync(path.join(root, "THIRD_PARTY_LICENSES.md"), "utf8")
    .replaceAll("](LICENSE)", "](PIPELINE_LICENSE.txt)"),
);
writeChanged(path.join(output, LIBRARY_NOTICE), libraryNotice({ root, version, official }));
writeChanged(
  path.join(output, "NOTICE.txt"),
  [
    `Pipeline ${version}`,
    "",
    "Pipeline's MIT license is in PIPELINE_LICENSE.txt.",
    "Third-party terms and source locations are in THIRD_PARTY_LICENSES.md.",
    `Rust and JavaScript library licenses are in ${LIBRARY_NOTICE}.`,
    "Poppler's license text is bundled beside the Poppler executables.",
    "",
  ].join("\n"),
);

console.log(`Prepared package notices for Pipeline ${version}`);
