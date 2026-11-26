// Pin the tested dependency closure, including delay imports. Unknown DLLs
// fail release packaging rather than being silently omitted or shipped.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const specification = JSON.parse(fs.readFileSync(new URL("./poppler-windows-files.json", import.meta.url)));
const lock = JSON.parse(fs.readFileSync(new URL("./poppler-lock.json", import.meta.url)));

export function peImports(bytes) {
  const need = (offset, size) => {
    if (!Number.isSafeInteger(offset) || offset < 0 || offset + size > bytes.length) throw new Error("Truncated PE image");
    return offset;
  };
  const u16 = offset => bytes.readUInt16LE(need(offset, 2));
  const u32 = offset => bytes.readUInt32LE(need(offset, 4));
  if (u16(0) !== 0x5a4d) throw new Error("Not a PE image");
  const pe = u32(0x3c);
  if (u32(pe) !== 0x4550 || u16(pe + 4) !== 0x8664) throw new Error("Expected Windows x64 PE image");
  const count = u16(pe + 6), optional = pe + 24, size = u16(pe + 20);
  if (u16(optional) !== 0x20b || size < 224 || count > 96) throw new Error("Unsupported PE headers");
  need(optional, size);
  const sections = Array.from({ length: count }, (_, i) => {
    const start = optional + size + i * 40;
    return { rva: u32(start + 12), bytes: u32(start + 16), offset: u32(start + 20) };
  });
  const offset = rva => {
    const section = sections.find(s => rva >= s.rva && rva - s.rva < s.bytes);
    if (!section) throw new Error("Invalid PE RVA");
    return need(section.offset + rva - section.rva, 1);
  };
  const imports = [];
  for (const [index, stride, nameOffset] of [[1, 20, 12], [13, 32, 4]]) {
    const rva = u32(optional + 112 + index * 8);
    const length = u32(optional + 116 + index * 8);
    if (!rva) continue;
    const start = offset(rva);
    need(start, length);
    let terminated = false;
    for (let item = start; item + stride <= start + length; item += stride) {
      const name = u32(item + nameOffset);
      if (!name) { terminated = true; break; }
      if (index === 13 && u32(item) !== 1) throw new Error("Unsupported non-RVA delay import");
      const first = offset(name), last = bytes.indexOf(0, first);
      if (last < first || last - first > 255) throw new Error("Invalid PE import name");
      const library = bytes.toString("ascii", first, last).toLowerCase();
      if (!/^[a-z0-9_.-]+\.dll$/.test(library)) throw new Error("Unsafe PE import name");
      imports.push(library);
    }
    if (!terminated) throw new Error("Unterminated PE import directory");
  }
  return imports;
}

export function selectedFiles(source) {
  if (specification.distributionVersion !== lock.windows.distributionVersion) throw new Error("Requalify the Poppler DLL list for this distribution");
  const files = new Map(fs.readdirSync(source).map(name => [name.toLowerCase(), name]));
  const queue = ["pdftotext.exe", "pdftoppm.exe"], included = new Set();
  while (queue.length) {
    const name = queue.pop();
    if (included.has(name)) continue;
    if (!specification.files.includes(name)) {
      if (/^api-ms-win-crt-[a-z0-9-]+\.dll$/.test(name) || specification.systemImports.includes(name)) continue;
      throw new Error(`Unqualified Poppler dependency: ${name}`);
    }
    const filename = files.get(name);
    if (!filename || !fs.lstatSync(path.join(source, filename)).isFile()) throw new Error(`Missing Poppler dependency: ${name}`);
    included.add(name);
    queue.push(...peImports(fs.readFileSync(path.join(source, filename))));
  }
  if (JSON.stringify([...included].sort()) !== JSON.stringify([...specification.files].sort())) throw new Error("Poppler dependency closure changed; requalify the pinned list");
  return [...included].sort().map(name => files.get(name));
}

export function stageWindowsPoppler(source, destination) {
  const files = selectedFiles(source);
  if (fs.existsSync(destination) && fs.readdirSync(destination).length) throw new Error("Poppler binary staging must be empty");
  fs.mkdirSync(destination, { recursive: true });
  for (const name of files) fs.copyFileSync(path.join(source, name), path.join(destination, name));
  return files;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [source, destination] = process.argv.slice(2);
  if (!source || !destination) throw new Error("Usage: poppler-windows.mjs <source-bin> <empty-destination>");
  console.log(`Staged ${stageWindowsPoppler(source, destination).length} qualified Poppler binaries`);
}
