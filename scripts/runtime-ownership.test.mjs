import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../gui/src-tauri/src/", import.meta.url));

function references(directory, pattern) {
  return fs
    .readdirSync(path.join(source, directory), {
      recursive: true,
      withFileTypes: true,
    })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .flatMap((entry) => {
      const file = path.join(entry.parentPath, entry.name);
      const relative = path.relative(source, file);
      if (
        relative.split(path.sep).includes("tests") ||
        /tests\.rs$/.test(entry.name)
      )
        return [];
      const lines = fs.readFileSync(file, "utf8").split("\n");
      return lines.flatMap((line, index) =>
        !line.trimStart().startsWith("//") && pattern.test(line)
          ? [`${relative}:${index + 1}`]
          : [],
      );
    });
}

test("Review code does not reach Workspace persistence or execution", () => {
  assert.deepEqual(
    references(
      "pipeline",
      /\bcrate::workbench::(?:store|commands|codex|api|claude)\b/,
    ),
    [],
  );
});

test("Workspace provider code does not reach Review persistence or scheduling", () => {
  for (const runtime of ["api", "claude", "codex"]) {
    assert.deepEqual(
      references(
        `workbench/${runtime}`,
        /\bcrate::(?:runs|commands::run_context|pipeline::executor)\b/,
      ),
      [],
    );
  }
});

// Expand Rust use trees, including aliases and multiline groups. This is a
// bounded import scanner, not a complete Rust parser or cfg evaluator.
function importPaths(code) {
  const paths = [];
  const clean = code
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");
  for (const statement of clean.matchAll(/\buse\s+([^;]+);/g)) {
    const tokens = statement[1].match(/[A-Za-z_]\w*|::|[{},*]/g) ?? [];
    let i = 0;
    const tree = (prefix) => {
      let parts = [...prefix];
      while (i < tokens.length) {
        const token = tokens[i++];
        if (token === "::") continue;
        if (token === "{") {
          while (i < tokens.length && tokens[i] !== "}") {
            tree(parts);
            if (tokens[i] === ",") i++;
          }
          i++;
          return;
        }
        if (token === "," || token === "}") {
          i--;
          break;
        }
        if (token === "as") {
          i++;
          break;
        }
        parts.push(token);
      }
      paths.push(parts.join("::"));
    };
    tree([]);
  }
  return paths;
}
function upwardImports(code) {
  return importPaths(code).filter((p) => /(?:^|::)commands(?:::|$)/.test(p));
}
test("runtime import scanner handles grouped imports, aliases and platform scopes", () => {
  assert.deepEqual(
    upwardImports(
      "use crate::workbench::{store::Store, commands::{run_store as read, SendTurnRequest}};",
    ),
    [
      "crate::workbench::commands::run_store",
      "crate::workbench::commands::SendTurnRequest",
    ],
  );
  assert.deepEqual(
    upwardImports(
      "#[cfg(windows)]\nuse crate::{commands::lifecycle, process};",
    ),
    ["crate::commands::lifecycle"],
  );
  assert.deepEqual(
    upwardImports(
      "use crate::workbench::{runtime::{run_store, contracts::SendTurnRequest}, store::Store};",
    ),
    [],
  );
});
test("runtime and process services have no upward IPC dependencies", () => {
  const files = [
    "process.rs",
    "process_tree.rs",
    "run_scope.rs",
    "cancellation.rs",
  ];
  for (const dir of [
    "workbench/runtime",
    "workbench/api",
    "workbench/claude",
    "workbench/codex",
  ])
    for (const entry of fs.readdirSync(path.join(source, dir), {
      recursive: true,
      withFileTypes: true,
    }))
      if (entry.isFile() && entry.name.endsWith(".rs"))
        files.push(
          path.relative(source, path.join(entry.parentPath, entry.name)),
        );
  for (const file of files) {
    const code = fs.readFileSync(path.join(source, file), "utf8");
    assert.deepEqual(upwardImports(code), [], file);
    assert.ok(!/crate::(?:workbench::)?commands::/.test(code), file);
  }
});
test("migrated runtime owners name their production imports", () => {
  for (const file of [
    "admission",
    "contracts",
    "storage",
    "submission",
    "finalization",
    "codex",
    "chat_approval",
    "session_state",
    "titles",
  ]) {
    const code = fs
      .readFileSync(
        path.join(source, "workbench/runtime", file + ".rs"),
        "utf8",
      )
      .split("#[cfg(test)]\nmod tests")[0];
    assert.ok(!/^use super::\*;/m.test(code), file);
  }
});
