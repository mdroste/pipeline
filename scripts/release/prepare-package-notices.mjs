#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { validateReleaseIdentity } from "./validate-release-identity.mjs";

const scriptPath = fileURLToPath(import.meta.url);
const root = path.resolve(path.dirname(scriptPath), "../..");
const version = validateReleaseIdentity({ rootDir: root });
const output = path.join(root, "gui", "src-tauri", "resources", "notices");

fs.mkdirSync(output, { recursive: true });
fs.copyFileSync(path.join(root, "LICENSE"), path.join(output, "PIPELINE_LICENSE.txt"));
fs.copyFileSync(
  path.join(root, "THIRD_PARTY_LICENSES.md"),
  path.join(output, "THIRD_PARTY_LICENSES.md"),
);
fs.writeFileSync(
  path.join(output, "NOTICE.txt"),
  [
    `Pipeline ${version}`,
    "",
    "Pipeline's MIT license is in PIPELINE_LICENSE.txt.",
    "Third-party terms and source locations are in THIRD_PARTY_LICENSES.md.",
    "Poppler's license text is bundled beside the Poppler executables.",
    "",
  ].join("\n"),
);

console.log(`Prepared package notices for Pipeline ${version}`);
