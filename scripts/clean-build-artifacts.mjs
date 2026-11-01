import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Deliberate allowlist: never infer that an ignored file is disposable.
const OUTPUTS = [
  "gui/src-tauri/target",
  "gui/src-tauri/gen/schemas",
  "gui/dist",
  "gui/.vite",
  "gui/tsconfig.tsbuildinfo",
  "gui/tsconfig.node.tsbuildinfo",
  "gui/node_modules/.vite",
  "gui/node_modules/.vite-temp",
];

function existsWithoutSymlinks(root, relative) {
  let current = root;
  for (const part of relative.split("/")) {
    current = path.join(current, part);
    let stat;
    try {
      stat = fs.lstatSync(current);
    } catch (error) {
      if (error.code === "ENOENT") return false;
      throw error;
    }
    if (stat.isSymbolicLink()) {
      throw new Error(`Refusing to traverse or remove symlink: ${current}`);
    }
  }
  return true;
}

export function cleanBuildArtifacts(root, { apply = false, worktrees = false, log = console.log } = {}) {
  root = fs.realpathSync(root);
  const checkouts = [""];
  const worktreeDir = ".claude/worktrees";
  if (worktrees && existsWithoutSymlinks(root, worktreeDir)) {
    for (const entry of fs.readdirSync(path.join(root, worktreeDir), { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      const relative = `${worktreeDir}/${entry.name}`;
      if (existsWithoutSymlinks(root, `${relative}/.git`)) checkouts.push(relative);
    }
  }

  // Validate the entire plan before deleting anything, including parent paths.
  const plan = checkouts.flatMap((checkout) => OUTPUTS.map((output) =>
    checkout ? `${checkout}/${output}` : output,
  )).filter((relative) => existsWithoutSymlinks(root, relative));

  if (!apply) log("Preview only. Stop builds/dev servers, then pass --apply to delete these generated outputs.");
  for (const relative of plan) {
    log(`${apply ? "Removing" : "Would remove"}: ${relative}`);
    if (apply) fs.rmSync(path.join(root, relative), { recursive: true, force: true });
  }
  if (plan.length === 0) log("No generated build outputs found.");
  return plan;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.some((arg) => !["--apply", "--worktrees"].includes(arg))) {
      throw new Error("Usage: node scripts/clean-build-artifacts.mjs [--apply] [--worktrees]");
    }
    cleanBuildArtifacts(path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."), {
      apply: args.includes("--apply"),
      worktrees: args.includes("--worktrees"),
    });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
