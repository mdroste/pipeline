import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { parseSync } from "vite";

const gui = fileURLToPath(new URL(".", import.meta.url));
const source = path.join(gui, "src");

function modules(file, code = fs.readFileSync(file, "utf8")) {
  const { program, errors } = parseSync(file, code);
  assert.equal(errors.length, 0, file);
  const imports = [];
  function visit(node) {
    if (
      [
        "ImportDeclaration",
        "ExportNamedDeclaration",
        "ExportAllDeclaration",
      ].includes(node.type) &&
      node.source?.type === "Literal"
    ) {
      const typeOnly =
        node.importKind === "type" ||
        node.exportKind === "type" ||
        (node.specifiers?.length > 0 &&
          node.specifiers.every(
            (item) => item.importKind === "type" || item.exportKind === "type",
          ));
      imports.push({
        specifier: node.source.value,
        kind: typeOnly ? "type" : "static",
      });
    } else if (
      node.type === "ImportExpression" &&
      node.source.type === "Literal"
    ) {
      imports.push({ specifier: node.source.value, kind: "dynamic" });
    } else if (
      node.type === "TSImportType" &&
      node.source?.type === "Literal"
    ) {
      imports.push({ specifier: node.source.value, kind: "type" });
    }
    for (const child of Object.values(node)) {
      if (Array.isArray(child)) {
        for (const element of child) if (element?.type) visit(element);
      } else if (child?.type) visit(child);
    }
  }
  visit(program);
  return imports;
}

function resolved(file, specifier) {
  // The internal source graph uses relative files and directory entry points.
  const base = path.resolve(path.dirname(file), specifier);
  const stem = base.replace(/\.jsx?$/, "");
  return [
    base,
    stem + ".ts",
    stem + ".tsx",
    path.join(base, "index.ts"),
    path.join(base, "index.tsx"),
  ].find((candidate) =>
    fs.statSync(candidate, { throwIfNoEntry: false })?.isFile(),
  );
}

function reachable(roots, accepts = () => true) {
  const seen = new Set();
  const pending = [...roots];
  while (pending.length) {
    const file = pending.pop();
    if (seen.has(file)) continue;
    seen.add(file);
    for (const edge of modules(file)) {
      if (!edge.specifier.startsWith(".") || !accepts(edge)) continue;
      const child = resolved(file, edge.specifier);
      if (child && /\.[cm]?tsx?$/.test(child) && !child.endsWith(".d.ts"))
        pending.push(child);
    }
  }
  return seen;
}

function scripts(directory) {
  return fs
    .readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(
      (entry) =>
        entry.isFile() &&
        /\.tsx?$/.test(entry.name) &&
        !entry.name.endsWith(".d.ts"),
    )
    .map((entry) => path.join(entry.parentPath, entry.name));
}

function htmlEntryPoints(directory) {
  return fs
    .readdirSync(directory)
    .filter((name) => name.endsWith(".html"))
    .flatMap((name) =>
      [
        ...fs
          .readFileSync(path.join(directory, name), "utf8")
          .matchAll(/<script\b[^>]*\bsrc="([^"]+\.[cm]?tsx?)"/g),
      ].map(([, src]) =>
        src.startsWith("/")
          ? path.join(gui, src.slice(1))
          : path.resolve(directory, src),
      ),
    );
}

function isTest(file) {
  return (
    /\.(?:test|spec)\.[cm]?tsx?$/.test(file) ||
    file.startsWith(path.join(source, "test") + path.sep)
  );
}

test("the module parser distinguishes lazy, type-only, and eager imports", () => {
  assert.deepEqual(
    modules(
      "fixture.ts",
      `
    import type { A } from "./types";
    export { B } from "./owner";
    const load = () => import("./lazy");
    type C = import("./other").C;
    const text = 'import("./decoy")';
  `,
    ),
    [
      { specifier: "./types", kind: "type" },
      { specifier: "./owner", kind: "static" },
      { specifier: "./lazy", kind: "dynamic" },
      { specifier: "./other", kind: "type" },
    ],
  );
});

test("frontend source files are reachable from the desktop, test, or browser fixture entries", () => {
  const files = scripts(source);
  const entryPoints = [
    ...htmlEntryPoints(gui),
    ...htmlEntryPoints(path.join(gui, "e2e")),
    ...files.filter((file) => /\.(?:test|spec)\.[cm]?tsx?$/.test(file)),
    path.join(source, "test/setup.ts"),
  ];
  const seen = reachable(entryPoints);
  assert.deepEqual(
    files
      .filter((file) => !seen.has(file))
      .map((file) => path.relative(gui, file)),
    [],
  );
});

test("route components and the PDF reader stay outside the initial import graph", () => {
  const eager = reachable(
    htmlEntryPoints(gui),
    (edge) => edge.kind === "static",
  );
  for (const feature of [
    "components/WorkspacePage.tsx",
    "components/PipelinePage.tsx",
    "components/ReviewRunView.tsx",
    "components/file-workspace/PdfReader.tsx",
  ])
    assert.ok(!eager.has(path.join(source, feature)), feature);
});

test("components and hooks send Tauri commands through feature clients", () => {
  const direct = scripts(source).filter(
    (file) =>
      ["components", "hooks"].includes(
        path.relative(source, file).split(path.sep)[0],
      ) &&
      !isTest(file) &&
      !file.endsWith(".test-support.ts") &&
      modules(file).some(
        ({ specifier }) => specifier === "@tauri-apps/api/core",
      ),
  );
  assert.deepEqual(
    direct.map((file) => path.relative(gui, file)),
    [],
  );
});
