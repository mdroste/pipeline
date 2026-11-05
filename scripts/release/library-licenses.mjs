// Builds the license file bundled with every installer. It covers the Rust
// crates and JavaScript packages the app can link or load, and reads their
// license text from the Cargo registry and node_modules, so it needs no download
// beyond what a build fetches anyway.
//
// The output must be identical on every machine and platform for one set of
// lockfiles: no paths, no timestamps, no locale-dependent sorting, and packages
// chosen from the whole lock rather than from what this platform installed.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

export const LIBRARY_NOTICE = "THIRD_PARTY_LIBRARIES.txt";

const LICENSE_FILE = /^(copying|copyright|licen[cs]e|notice|unlicense)([._-].*)?$/i;
const SOURCE_FILE = /\.(c|cjs|css|d\.ts|h|html|js|json|map|mjs|py|rs|sh|toml|ts|ya?ml)$/i;
const MAX_LICENSE_BYTES = 1024 * 1024;
// pdf.js keeps the licenses of its fonts and decoders beside them, and the build
// copies those fonts and decoders into the app.
const LICENSE_SUBFOLDERS = { "pdfjs-dist": ["standard_fonts", "wasm"] };

const compare = (left, right) => (left < right ? -1 : left > right ? 1 : 0);
const label = (item) => `${item.ecosystem} ${item.name} ${item.version}`;
const normalize = (text) => text.replace(/^﻿/, "").replaceAll("\r\n", "\n").trim();

function licenseFiles(folder) {
  if (!fs.existsSync(folder)) return [];
  return fs.readdirSync(folder, { withFileTypes: true })
    .filter((entry) => entry.isFile() && LICENSE_FILE.test(entry.name) && !SOURCE_FILE.test(entry.name))
    .map((entry) => path.join(folder, entry.name));
}

/** Reads the license texts a package publishes in its own files. */
export function readLicenseTexts(directory, { explicit, subfolders = [] } = {}) {
  const files = new Set(licenseFiles(directory));
  for (const folder of subfolders) {
    for (const file of licenseFiles(path.join(directory, folder))) files.add(file);
  }
  if (explicit && fs.existsSync(explicit)) files.add(path.resolve(explicit));
  return [...files]
    .filter((file) => fs.statSync(file).size <= MAX_LICENSE_BYTES)
    .map((file) => normalize(fs.readFileSync(file, "utf8")))
    .filter(Boolean);
}

/** Crates reachable from the app through normal dependencies, for every target. */
export function cargoProductionPackages(metadata) {
  const packages = new Map(metadata.packages.map((item) => [item.id, item]));
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]));
  const reached = new Set();
  const pending = [metadata.resolve.root];
  while (pending.length) {
    const id = pending.pop();
    if (reached.has(id)) continue;
    reached.add(id);
    for (const dependency of nodes.get(id)?.deps ?? []) {
      if (dependency.dep_kinds.some((item) => item.kind === null)) pending.push(dependency.pkg);
    }
  }
  return [...reached]
    .map((id) => packages.get(id))
    .filter((item) => item?.source)
    .map((item) => {
      const directory = path.dirname(item.manifest_path);
      return {
        ecosystem: "crate",
        name: item.name,
        version: item.version,
        license: item.license ?? null,
        authors: item.authors ?? [],
        repository: item.repository ?? null,
        directory,
        explicit: item.license_file ? path.resolve(directory, item.license_file) : undefined,
      };
    });
}

function npmName(lockPath, entry) {
  if (entry.name) return entry.name;
  const marker = "node_modules/";
  const index = lockPath.lastIndexOf(marker);
  return index >= 0 ? lockPath.slice(index + marker.length) : undefined;
}

function npmAuthors(directory) {
  const manifest = path.join(directory, "package.json");
  if (!fs.existsSync(manifest)) return [];
  const { author } = JSON.parse(fs.readFileSync(manifest, "utf8"));
  const name = typeof author === "string" ? author : author?.name;
  return name ? [name] : [];
}

// Optional entries are Node add-ons for one platform. A web bundle can't load
// them, and leaving them out keeps the file the same on every platform.
/** Packages that `npm ci --omit=dev` would install, one entry per version. */
export function npmProductionPackages(lock, guiDirectory) {
  const packages = new Map();
  for (const [lockPath, entry] of Object.entries(lock.packages ?? {})) {
    if (!lockPath || !entry?.version || entry.dev || entry.devOptional || entry.optional) continue;
    const name = npmName(lockPath, entry);
    if (!name) continue;
    const key = `${name}@${entry.version}`;
    if (packages.has(key)) continue;
    const directory = path.join(guiDirectory, lockPath);
    packages.set(key, {
      ecosystem: "npm",
      name,
      version: entry.version,
      license: entry.license ?? null,
      authors: npmAuthors(directory),
      repository: null,
      directory,
    });
  }
  return [...packages.values()];
}

/** Adds each package's license text. */
export function withLicenseTexts(packages) {
  return packages.map((item) => ({
    ...item,
    texts: readLicenseTexts(item.directory, {
      explicit: item.explicit,
      subfolders: LICENSE_SUBFOLDERS[item.name],
    }),
  }));
}

/** Packages an official build can't ship: no files on disk, or no license at all. */
export function unlicensedPackages(packages) {
  return packages
    .filter((item) => !fs.existsSync(item.directory) || (!item.license && !item.texts.length))
    .map(label)
    .sort(compare);
}

export function renderLibraryNotice(version, packages) {
  const sorted = [...packages].sort((left, right) => compare(label(left), label(right)));
  const groups = new Map();
  for (const item of sorted) {
    for (const text of new Set(item.texts)) {
      if (!groups.has(text)) groups.set(text, []);
      groups.get(text).push(label(item));
    }
  }
  const lines = [
    `Third-party libraries in Pipeline ${version}`,
    "",
    "The first list names every Rust crate and JavaScript package the app can",
    "include, for all platforms. License text follows, one copy for all the",
    "packages that share it. A package offered under a choice of licenses is",
    "listed with each text it publishes.",
    "",
    "Libraries",
    "---------",
    ...sorted.map((item) => `${label(item)}: ${item.license ?? "see license text"}`),
  ];
  const withoutText = sorted.filter((item) => !item.texts.length);
  if (withoutText.length) {
    lines.push(
      "",
      "Published without a license file",
      "--------------------------------",
      "These packages state their license in their metadata and include no text.",
      "",
      ...withoutText.map((item) => [
        `${label(item)}: ${item.license ?? "no license stated"}`,
        ...(item.authors.length ? [`  Authors: ${item.authors.join(", ")}`] : []),
        ...(item.repository ? [`  Source: ${item.repository}`] : []),
      ].join("\n")),
    );
  }
  lines.push("", "License texts", "-------------");
  const ordered = [...groups.entries()].sort(([, left], [, right]) => compare(left[0], right[0]));
  for (const [text, users] of ordered) {
    lines.push("", "=".repeat(78), `Used by: ${users.join(", ")}`, "=".repeat(78), "", text);
  }
  return `${lines.join("\n")}\n`;
}

function cargoMetadata(tauriDirectory) {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--locked", "--manifest-path", path.join(tauriDirectory, "Cargo.toml")],
    { cwd: tauriDirectory, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  if (result.status !== 0) {
    throw new Error(`cargo metadata failed: ${(result.stderr || result.error?.message || "unknown error").trim()}`);
  }
  return JSON.parse(result.stdout);
}

/** Lists every shipped library. Each ecosystem must be readable for this to work. */
export function collectLibraries(root) {
  const gui = path.join(root, "gui");
  const lock = JSON.parse(fs.readFileSync(path.join(gui, "package-lock.json"), "utf8"));
  return withLicenseTexts([
    ...cargoProductionPackages(cargoMetadata(path.join(gui, "src-tauri"))),
    ...npmProductionPackages(lock, gui),
  ]);
}

/**
 * The text of the bundled file. Development builds need the file to exist but
 * must not depend on a network or a full install, so only official builds fail.
 */
export function libraryNotice({ root, version, official }) {
  let libraries;
  try {
    libraries = collectLibraries(root);
  } catch (error) {
    if (official) throw error;
    console.warn(`Library license notice is incomplete: ${error.message}`);
    return `Third-party libraries in Pipeline ${version}\n\nThis development build has no library list.\n`;
  }
  const unlicensed = unlicensedPackages(libraries);
  if (unlicensed.length) {
    const message = `Packages without license evidence: ${unlicensed.join(", ")}`;
    if (official) throw new Error(message);
    console.warn(message);
  }
  return renderLibraryNotice(version, libraries);
}
