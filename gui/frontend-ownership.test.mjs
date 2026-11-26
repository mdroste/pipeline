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
      node.type === "NewExpression" &&
      node.callee.type === "Identifier" &&
      ["Worker", "SharedWorker"].includes(node.callee.name)
    ) {
      const url = node.arguments[0];
      const base = url?.arguments?.[1];
      if (
        url?.type === "NewExpression" &&
        url.callee.type === "Identifier" &&
        url.callee.name === "URL" &&
        url.arguments[0]?.type === "Literal" &&
        typeof url.arguments[0].value === "string" &&
        base?.type === "MemberExpression" &&
        !base.computed &&
        base.property.name === "url" &&
        base.object.type === "MetaProperty" &&
        base.object.meta.name === "import" &&
        base.object.property.name === "meta"
      ) {
        imports.push({ specifier: url.arguments[0].value, kind: "worker" });
      }
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

function contractViolations(file, code) {
  return modules(file, code).filter(
    ({ specifier, kind }) =>
      kind !== "type" ||
      /(?:Client$|components\/|hooks\/|@tauri-apps)/.test(specifier),
  );
}

test("project contracts depend only on contract types", () => {
  for (const name of ["projectTypes", "studioTypes", "deskTypes"])
    assert.deepEqual(
      contractViolations(path.join(source, `lib/${name}.ts`)),
      [],
      name,
    );
});

test("contract ownership rejects transport, UI and eager dependencies", () => {
  assert.equal(
    contractViolations("fixture.ts", 'import { x } from "./types";').length,
    1,
  );
  assert.equal(
    contractViolations(
      "fixture.ts",
      'import type { X } from "./projectClient";',
    ).length,
    1,
  );
  assert.equal(
    contractViolations("fixture.ts", 'import type { X } from "./projectTypes";')
      .length,
    0,
  );
});

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

test("worker URLs are entry points, while unrelated URLs and strings are not", () => {
  assert.deepEqual(
    modules(
      "fixture.ts",
      `
      new Worker(new URL("./dedicated.ts", import.meta.url), { type: "module" });
      new SharedWorker(new URL("./shared.ts", import.meta.url));
      new URL("./asset.ts", import.meta.url);
      new Worker(new URL("./other-base.ts", location.href));
      new Worker(new URL(dynamicPath, import.meta.url));
      const decoy = 'new Worker(new URL("./decoy.ts", import.meta.url))';
    `,
    ),
    [
      { specifier: "./dedicated.ts", kind: "worker" },
      { specifier: "./shared.ts", kind: "worker" },
    ],
  );
});

test("frontend source files are reachable from the desktop, test, or browser fixture entries", (t) => {
  const files = scripts(source);
  const entryPoints = [
    ...htmlEntryPoints(gui),
    ...htmlEntryPoints(path.join(gui, "e2e")),
    ...files.filter((file) => /\.(?:test|spec)\.[cm]?tsx?$/.test(file)),
    path.join(source, "test/setup.ts"),
  ];
  const seen = reachable(entryPoints);
  const desktop = reachable(htmlEntryPoints(gui));
  const fixtures = reachable(htmlEntryPoints(path.join(gui, "e2e")));
  t.diagnostic(
    JSON.stringify({
      desktop: files.filter((file) => desktop.has(file)).length,
      fixtureOnly: files
        .filter((file) => !desktop.has(file) && fixtures.has(file))
        .map((file) => path.relative(gui, file)),
      testOnly: files.filter(
        (file) => !desktop.has(file) && !fixtures.has(file) && seen.has(file),
      ).length,
    }),
  );
  assert.deepEqual(
    files
      .filter((file) => !seen.has(file))
      .map((file) => path.relative(gui, file)),
    [],
  );
});

// Report strongly connected components, so one dependency cycle has one stable
// description rather than one message for every traversal through it.
function cycles(graph) {
  let next = 0;
  const indexes = new Map(),
    low = new Map(),
    active = new Set(),
    stack = [],
    found = [];
  function visit(file) {
    indexes.set(file, next);
    low.set(file, next++);
    stack.push(file);
    active.add(file);
    for (const dependency of graph.get(file) ?? []) {
      if (!graph.has(dependency)) continue;
      if (!indexes.has(dependency)) {
        visit(dependency);
        low.set(file, Math.min(low.get(file), low.get(dependency)));
      } else if (active.has(dependency))
        low.set(file, Math.min(low.get(file), indexes.get(dependency)));
    }
    if (low.get(file) !== indexes.get(file)) return;
    const component = [];
    let member;
    do {
      member = stack.pop();
      active.delete(member);
      component.push(member);
    } while (member !== file);
    if (component.length > 1 || graph.get(file)?.includes(file))
      found.push(component.sort());
  }
  for (const file of graph.keys()) if (!indexes.has(file)) visit(file);
  return found.sort((a, b) => a[0].localeCompare(b[0]));
}

test("cycle reporting distinguishes a shared dependency from a circular dependency", () => {
  assert.deepEqual(
    cycles(
      new Map([
        ["page", ["model", "client"]],
        ["model", ["client"]],
        ["client", []],
      ]),
    ),
    [],
  );
  assert.deepEqual(
    cycles(
      new Map([
        ["page", ["model"]],
        ["model", ["client"]],
        ["client", ["model"]],
      ]),
    ),
    [["client", "model"]],
  );
});

test("production frontend runtime dependencies are acyclic", () => {
  const graph = new Map(
    scripts(source)
      .filter((file) => !isTest(file))
      .map((file) => [
        file,
        modules(file)
          .filter(
            (edge) => edge.kind === "static" && edge.specifier.startsWith("."),
          )
          .map((edge) => resolved(file, edge.specifier))
          .filter(Boolean),
      ]),
  );
  assert.deepEqual(
    cycles(graph).map((component) =>
      component.map((file) => path.relative(gui, file)),
    ),
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

test("conversation transport adapters consume intents, not model internals", () => {
  for (const name of [
    "createWorkspaceSubmission.ts",
    "useWorkspaceConversationEvents.ts",
  ]) {
    const file = path.join(source, "hooks/workspace", name);
    assert.ok(
      !modules(file).some(({ specifier }) =>
        specifier.endsWith("useWorkspaceConversationRuntime"),
      ),
      name,
    );
    assert.ok(
      !/submissionRef|activeRef|setActive|setSubmitting/.test(
        fs.readFileSync(file, "utf8"),
      ),
      name,
    );
  }
});
