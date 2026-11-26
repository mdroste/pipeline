import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { createRequire } from "node:module";
import { fileURLToPath, pathToFileURL } from "node:url";

const requireGui = createRequire(
  new URL("../gui/package.json", import.meta.url),
);
const { parseSync } = await import(
  pathToFileURL(requireGui.resolve("vite")).href
);

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

function registeredCommands() {
  const lib = fs.readFileSync(
    path.join(ROOT, "gui/src-tauri/src/lib.rs"),
    "utf8",
  );
  const handler = lib.match(/generate_handler!\[([\s\S]*?)\]/)?.[1];
  assert.ok(handler, "lib.rs must register commands with generate_handler!");
  return handler
    .split(",")
    .map((entry) => entry.trim().split("::").pop())
    .filter(Boolean);
}

function interfaceSource(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name);
    if (entry.isDirectory()) return interfaceSource(file);
    return /\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name)
      ? [{ file, code: fs.readFileSync(file, "utf8") }]
      : [];
  });
}

function invokedCommands(file, code) {
  const { program, errors } = parseSync(file, code);
  assert.equal(errors.length, 0, file);
  const named = new Set();
  const namespaces = new Set();
  const tables = new Map();
  for (const node of program.body) {
    // Literal dispatch tables count only when an invoke indexes that table.
    // An unrelated string constant must not keep a retired command alive.
    if (node.type === "VariableDeclaration" && node.kind === "const") {
      for (const declaration of node.declarations) {
        if (
          declaration.id.type === "Identifier" &&
          declaration.init?.type === "ObjectExpression"
        ) {
          const values = declaration.init.properties.map(
            (property) => property.value,
          );
          if (
            values.every(
              (value) =>
                value?.type === "Literal" && typeof value.value === "string",
            )
          )
            tables.set(
              declaration.id.name,
              values.map((value) => value.value),
            );
        }
      }
    }
    if (
      node.type !== "ImportDeclaration" ||
      node.source.value !== "@tauri-apps/api/core"
    )
      continue;
    for (const specifier of node.specifiers) {
      if (
        specifier.type === "ImportSpecifier" &&
        specifier.imported.name === "invoke"
      )
        named.add(specifier.local.name);
      if (specifier.type === "ImportNamespaceSpecifier")
        namespaces.add(specifier.local.name);
    }
  }
  const commands = [];
  function visit(node) {
    if (node.type === "CallExpression") {
      const call = node.callee;
      const isInvoke =
        (call.type === "Identifier" && named.has(call.name)) ||
        (call.type === "MemberExpression" &&
          call.object.type === "Identifier" &&
          namespaces.has(call.object.name) &&
          !call.computed &&
          call.property.name === "invoke");
      const argument = node.arguments[0];
      if (
        isInvoke &&
        argument?.type === "Literal" &&
        typeof argument.value === "string"
      )
        commands.push(argument.value);
      if (
        isInvoke &&
        argument?.type === "TemplateLiteral" &&
        argument.expressions.length === 0
      )
        commands.push(argument.quasis[0].value.cooked);
      if (
        isInvoke &&
        argument?.type === "MemberExpression" &&
        argument.computed &&
        argument.object.type === "Identifier"
      )
        commands.push(...(tables.get(argument.object.name) ?? []));
    }
    for (const child of Object.values(node)) {
      if (Array.isArray(child)) {
        for (const item of child) if (item?.type) visit(item);
      } else if (child?.type) visit(child);
    }
  }
  visit(program);
  return commands;
}

test("command callers are parsed invokes, including aliases and namespace imports", () => {
  assert.deepEqual(
    invokedCommands(
      "fixture.ts",
      `
    import { invoke as native } from "@tauri-apps/api/core";
    import * as core from "@tauri-apps/api/core";
    native<{ value: string }>("first_command", {});
    core.invoke("second_command");
    const formats: Record<string, string> = { html: "export_html", pdf: "export_pdf" };
    native(formats[format]);
    const unused = { old: "retired_export" };
    // invoke("not_a_caller")
    const retired = "retired_command";
    log("also_not_a_caller");
  `,
    ),
    ["first_command", "second_command", "export_html", "export_pdf"],
  );
});

test("every registered backend command has an invoke caller in the interface", () => {
  const invoked = new Set(
    interfaceSource(path.join(ROOT, "gui/src")).flatMap(({ file, code }) =>
      invokedCommands(file, code),
    ),
  );
  const registered = registeredCommands();
  assert.ok(registered.length > 100, "the registration list must parse");
  assert.deepEqual(
    registered.filter((name) => !invoked.has(name)),
    [],
  );
  assert.deepEqual(
    [...invoked].filter((name) => !registered.includes(name)),
    [],
    "interface invokes must name registered native commands",
  );
});

// Only Tauri wrappers are inventoried. Headless entry points and CLI helpers
// are ordinary Rust functions and have no Tauri attribute.
function declaredCommands(directory) {
  const pattern =
    /^\s*#\[tauri::command(?:\([^\n]*\))?\]\s*(?:#\[[^\n]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+(\w+)/gm;
  return fs
    .readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .flatMap((entry) =>
      [
        ...fs
          .readFileSync(path.join(entry.parentPath, entry.name), "utf8")
          .matchAll(pattern),
      ].map(([, name]) => name),
    );
}

test("every Tauri command declaration has a registration", () => {
  const registered = registeredCommands().sort();
  const declared = declaredCommands(
    path.join(ROOT, "gui/src-tauri/src"),
  ).sort();
  assert.deepEqual(declared, registered);
});
