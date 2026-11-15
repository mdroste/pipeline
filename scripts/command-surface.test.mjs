import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// Still registered with no caller in the interface. Remove each one, with this
// entry, as its own reviewable change.
const AWAITING_REMOVAL = ["workflow_codex_login_start", "workflow_codex_login_cancel"];

function registeredCommands() {
  const lib = fs.readFileSync(path.join(ROOT, "gui/src-tauri/src/lib.rs"), "utf8");
  const handler = lib.match(/generate_handler!\[([\s\S]*?)\]/)?.[1];
  assert.ok(handler, "lib.rs must register commands with generate_handler!");
  return handler.split(",").map((entry) => entry.trim().split("::").pop()).filter(Boolean);
}

function interfaceSource(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) return interfaceSource(file);
    return /\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name) ? [fs.readFileSync(file, "utf8")] : [];
  });
}

test("every registered backend command has a caller in the interface", () => {
  const source = interfaceSource(path.join(ROOT, "gui/src")).join("\n");
  const quoted = new Set([...source.matchAll(/["'`]([a-z][a-z0-9_]*)["'`]/g)].map((match) => match[1]));
  const registered = registeredCommands();
  assert.ok(registered.length > 100, "the registration list must parse");
  assert.deepEqual(
    registered.filter((name) => !quoted.has(name) && !AWAITING_REMOVAL.includes(name)),
    [],
  );
  for (const name of AWAITING_REMOVAL) assert.ok(registered.includes(name), name);
  for (const name of AWAITING_REMOVAL) assert.ok(!quoted.has(name), `${name} has a caller again`);
});

// Only Tauri wrappers are inventoried. Headless entry points and CLI helpers
// are ordinary Rust functions and have no Tauri attribute.
function declaredCommands(directory) {
  const pattern = /^\s*#\[tauri::command(?:\([^\n]*\))?\]\s*(?:#\[[^\n]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)/gm;
  return fs.readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .flatMap((entry) => [...fs.readFileSync(path.join(entry.parentPath, entry.name), "utf8")
      .matchAll(pattern)].map(([, name]) => name));
}

test("every Tauri command declaration has a registration", () => {
  const registered = registeredCommands().sort();
  const declared = declaredCommands(path.join(ROOT, "gui/src-tauri/src")).sort();
  assert.deepEqual(declared, registered);
});
