import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

// Built at runtime from a template, so no literal name appears in the source.
const TEMPLATED = ["workbench_shell_status", "workbench_shell_install",
  "workbench_browser_status", "workbench_browser_install"];
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
    registered.filter((name) => !quoted.has(name) && !TEMPLATED.includes(name) && !AWAITING_REMOVAL.includes(name)),
    [],
  );
  for (const name of [...TEMPLATED, ...AWAITING_REMOVAL]) assert.ok(registered.includes(name), name);
  for (const name of AWAITING_REMOVAL) assert.ok(!quoted.has(name), `${name} has a caller again`);
});
