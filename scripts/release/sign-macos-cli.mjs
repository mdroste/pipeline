#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export function cliSigningPlan({ platform, env, root, exists = fs.existsSync }) {
  if (platform !== "darwin") return null;
  const identity = env.APPLE_SIGNING_IDENTITY;
  if (!identity) {
    if (env.PIPELINE_OFFICIAL_RELEASE === "1")
      throw new Error("APPLE_SIGNING_IDENTITY is required for an official release");
    return null;
  }
  const targetRoot = path.resolve(root, env.CARGO_TARGET_DIR || "target");
  const triple = env.TAURI_ENV_TARGET_TRIPLE || env.CARGO_BUILD_TARGET;
  const candidates = env.PIPELINE_CLI_BINARY
    ? [path.resolve(root, env.PIPELINE_CLI_BINARY)]
    : triple
      ? [path.join(targetRoot, triple, "release", "pipeline-cli")]
      : [path.join(targetRoot, "release", "pipeline-cli")];
  const binary = candidates.find(exists);
  if (!binary) throw new Error(`pipeline-cli was not found at ${candidates.join(", ")}`);
  return { identity, binary };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../gui/src-tauri");
  const plan = cliSigningPlan({ platform: process.platform, env: process.env, root });
  if (!plan) console.log("Skipping CLI signing (local unsigned build or non-macOS platform)");
  else {
    execFileSync("codesign", ["--force", "--options", "runtime", "--timestamp", "--sign", plan.identity, plan.binary], { stdio: "inherit" });
    console.log(`Signed ${plan.binary}`);
  }
}
