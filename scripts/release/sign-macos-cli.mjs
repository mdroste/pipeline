#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

if (process.platform !== "darwin") {
  console.log("Skipping macOS CLI signing on this platform");
  process.exit(0);
}

const identity = process.env.APPLE_SIGNING_IDENTITY;
if (!identity) throw new Error("APPLE_SIGNING_IDENTITY is required to sign pipeline-cli");

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../gui/src-tauri");
const targetRoot = path.join(root, "target");
const binaries = fs
  .readdirSync(targetRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && entry.name.endsWith("-apple-darwin"))
  .map((entry) => path.join(targetRoot, entry.name, "release", "pipeline-cli"))
  .filter((candidate) => fs.existsSync(candidate));

if (binaries.length === 0) {
  throw new Error(`pipeline-cli was not found under ${targetRoot}`);
}

for (const binary of binaries) {
  execFileSync(
    "codesign",
    ["--force", "--options", "runtime", "--timestamp", "--sign", identity, binary],
    { stdio: "inherit" },
  );
  console.log(`Signed ${binary}`);
}
