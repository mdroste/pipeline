// Compare original and trimmed tools against the committed PDF corpus. Child
// PATH/cwd exclude ambient Poppler libraries; preserve normal Windows runtime DLLs.
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { popplerBinary } from "./poppler-inventory.mjs";

export function qualifyPoppler(original, trimmed) {
  const fixtures = fileURLToPath(new URL("./fixtures/poppler/", import.meta.url));
  const cases = JSON.parse(fs.readFileSync(path.join(fixtures, "cases.json")));
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline PDF qualification "));
  let pages = 0;
  try {
    // Keep the observed relative layout but run copies under fresh ancestors.
    // A neighboring share/poppler directory must not rescue a broken package.
    [original, trimmed] = [original, trimmed].map((root, index) => {
      const isolated = path.join(temporary, "isolated", String(index), "poppler");
      const directory = path.dirname(popplerBinary(root, "pdftotext"));
      const relative = path.relative(root, directory);
      if (path.isAbsolute(relative) || relative.startsWith("..")) throw new Error("Tools must be inside the Poppler package");
      fs.cpSync(directory, path.join(isolated, relative), { recursive: true });
      if (relative) fs.cpSync(path.join(root, "share"), path.join(isolated, "share"), { recursive: true });
      return isolated;
    });
    for (const [index, entry] of cases.entries()) {
      const input = path.join(fixtures, entry.file);
      const password = entry.password ? ["-upw", entry.password] : [];
      const run = (root, tool, args) => {
        const data = [path.join(root, "share/poppler"), path.resolve(root, "../../share/poppler")].find(directory => fs.existsSync(directory));
        if (!data) throw new Error(`Missing Poppler language data: ${root}`);
        const env = { ...process.env, POPPLER_DATADIR: data };
        if (process.platform === "win32") env.PATH = `${process.env.SystemRoot || "C:\\Windows"}\\System32`;
        const result = spawnSync(popplerBinary(root, tool), [...password, ...args], { cwd: temporary, env, timeout: 30_000, maxBuffer: 16 * 1024 * 1024, windowsHide: true });
        if (result.error) throw result.error;
        return result;
      };
      for (const mode of ["-layout", "-bbox-layout"]) {
        const before = run(original, "pdftotext", [mode, input, "-"]);
        const after = run(trimmed, "pdftotext", [mode, input, "-"]);
        assert.equal(after.status, before.status, `${entry.file}: ${mode} exit code`);
        if (entry.fails) { assert.notEqual(after.status, 0); continue; }
        assert.equal(after.status, 0, after.stderr.toString());
        assert.deepEqual(after.stdout, before.stdout, `${entry.file}: ${mode} output`);
        if (mode === "-layout") assert.ok(after.stdout.toString().includes(entry.text), `${entry.file}: expected text`);
      }
      const render = (root, label) => {
        const prefix = path.join(temporary, `${index}-${label}`);
        const result = run(root, "pdftoppm", ["-r", "72", "-png", "-cropbox", input, prefix]);
        return { result, images: fs.readdirSync(temporary).filter(f => f.startsWith(`${index}-${label}-`) && f.endsWith(".png")).sort() };
      };
      const before = render(original, "original"), after = render(trimmed, "trimmed");
      assert.equal(after.result.status, before.result.status, `${entry.file}: renderer exit code`);
      if (entry.fails) { assert.notEqual(after.result.status, 0); continue; }
      assert.equal(after.result.status, 0, after.result.stderr.toString());
      assert.ok(after.images.length > 0, `${entry.file}: rendered pages`);
      assert.equal(after.images.length, before.images.length);
      for (let page = 0; page < after.images.length; page++) {
        const actual = fs.readFileSync(path.join(temporary, after.images[page]));
        assert.deepEqual(actual.subarray(0, 8), Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
        assert.deepEqual(actual, fs.readFileSync(path.join(temporary, before.images[page])), `${entry.file}: rendered page ${page + 1}`);
        pages++;
      }
    }
    console.log(`Poppler qualification passed: ${cases.length} cases, ${pages} identical PNG pages; text and bounding boxes identical; encrypted/damaged failures preserved`);
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) qualifyPoppler(path.resolve(process.argv[2]), path.resolve(process.argv[3]));
