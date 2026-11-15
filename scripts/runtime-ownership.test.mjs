import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../gui/src-tauri/src/", import.meta.url));

function references(directory, pattern) {
  return fs.readdirSync(path.join(source, directory), { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .flatMap((entry) => {
      const file = path.join(entry.parentPath, entry.name);
      const relative = path.relative(source, file);
      if (relative.split(path.sep).includes("tests") || /tests\.rs$/.test(entry.name)) return [];
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
    references("pipeline", /\bcrate::workbench::(?:store|commands|codex|api|claude)\b/),
    [],
  );
});

test("Workspace provider code does not reach Review persistence or scheduling", () => {
  for (const runtime of ["api", "claude", "codex"]) {
    assert.deepEqual(
      references(`workbench/${runtime}`, /\bcrate::(?:runs|commands::run_context|pipeline::executor)\b/),
      [],
    );
  }
});
