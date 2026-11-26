import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { peImports, selectedFiles, stageWindowsPoppler } from "./poppler-windows.mjs";
import { popplerBinary } from "./poppler-inventory.mjs";
import { verifyCliLauncher } from "./verify-cli-launcher.mjs";
import { resolvePython } from "./python.mjs";

const root = fileURLToPath(new URL("../../", import.meta.url));
const specification = JSON.parse(fs.readFileSync(new URL("./poppler-windows-files.json", import.meta.url)));

function pe(names, delay = false) {
  const bytes = Buffer.alloc(4096), header = 0x80, optional = header + 24;
  bytes.writeUInt16LE(0x5a4d, 0); bytes.writeUInt32LE(header, 0x3c);
  bytes.writeUInt32LE(0x4550, header); bytes.writeUInt16LE(0x8664, header + 4);
  bytes.writeUInt16LE(1, header + 6); bytes.writeUInt16LE(224, header + 20);
  bytes.writeUInt16LE(0x20b, optional);
  const section = optional + 224;
  bytes.writeUInt32LE(0x1000, section + 12); bytes.writeUInt32LE(0xe00, section + 16);
  bytes.writeUInt32LE(0x200, section + 20);
  const stride = delay ? 32 : 20, directory = optional + 112 + (delay ? 13 : 1) * 8;
  bytes.writeUInt32LE(0x1000, directory); bytes.writeUInt32LE((names.length + 1) * stride, directory + 4);
  let nameOffset = 0x700;
  names.forEach((name, i) => {
    const descriptor = 0x200 + i * stride;
    if (delay) bytes.writeUInt32LE(1, descriptor);
    bytes.writeUInt32LE(nameOffset + 0xe00, descriptor + (delay ? 4 : 12));
    bytes.write(name + "\0", nameOffset); nameOffset += name.length + 1;
  });
  return bytes;
}

test("Poppler dependency reader handles normal/delay imports and rejects malformed images", () => {
  for (const delay of [false, true]) assert.deepEqual(peImports(pe(["KERNEL32.dll", "poppler.dll"], delay)), ["kernel32.dll", "poppler.dll"]);
  assert.throws(() => peImports(Buffer.alloc(1)), /Truncated/);
  assert.throws(() => peImports(pe(["../outside.dll"])), /Unsafe/);
  const badRva = pe(["poppler.dll"]); badRva.writeUInt32LE(0xffffff, 0x20c);
  assert.throws(() => peImports(badRva), /RVA/);
  const badDelay = pe(["poppler.dll"], true); badDelay.writeUInt32LE(0, 0x200);
  assert.throws(() => peImports(badDelay), /delay import/);
});

test("Poppler staging includes only the qualified closure and rejects missing/new dependencies", () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline DLL closure "));
  try {
    const source = path.join(temporary, "source"), destination = path.join(temporary, "poppler/Library/bin");
    fs.mkdirSync(source);
    const libraries = specification.files.filter(name => name.endsWith(".dll"));
    for (const name of specification.files) fs.writeFileSync(path.join(source, name), pe(name.endsWith(".exe") ? libraries : ["kernel32.dll"]));
    fs.writeFileSync(path.join(source, "unused.dll"), "must not ship");
    assert.deepEqual(selectedFiles(source), [...specification.files].sort());
    stageWindowsPoppler(source, destination);
    assert.equal(fs.readdirSync(destination).length, specification.files.length);
    assert.equal(popplerBinary(path.resolve(destination, "../.."), "pdftotext", true), path.join(destination, "pdftotext.exe"));
    assert.throws(() => stageWindowsPoppler(source, destination), /empty/);
    fs.writeFileSync(path.join(source, "poppler.dll"), pe(["unknown.dll"], true));
    assert.throws(() => selectedFiles(source), /Unqualified/);
    fs.rmSync(path.join(source, "poppler.dll"));
    assert.throws(() => selectedFiles(source), /Missing/);
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
});

test("packaged CLI forwards arguments, streams and exit codes at old and user-installed paths", () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline CLI ü fixture "));
  const exe = process.platform === "win32" ? ".exe" : "";
  try {
    const compile = (source, binary) => {
      const result = spawnSync("rustc", ["--edition=2021", source, "-o", binary], { encoding: "utf8", windowsHide: true, timeout: 60_000 });
      assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
    };
    const source = path.join(temporary, "launcher.rs"), bundled = path.join(temporary, `pipeline-cli${exe}`);
    fs.writeFileSync(source, `#[path = ${JSON.stringify(path.join(root, "gui/src-tauri/src/cli/launcher.rs"))}] mod launcher; fn main() { launcher::main(); }`);
    compile(source, bundled);
    const host = path.join(temporary, "host.rs"), app = path.join(temporary, `pipeline-gui${exe}`);
    fs.writeFileSync(host, `use std::io::Read; fn main() { for arg in std::env::args().skip(1) { println!("arg={arg}"); } let mut input=String::new(); std::io::stdin().read_to_string(&mut input).unwrap(); print!("{input}"); eprintln!("expected stderr"); std::process::exit(23); }`);
    compile(host, app);
    const user = path.join(temporary, "user bin"); fs.mkdirSync(user);
    const installed = path.join(user, `pipeline-cli${exe}`);
    fs.copyFileSync(bundled, installed);
    fs.writeFileSync(path.join(user, "pipeline-cli-app"), app);
    for (const binary of [bundled, installed]) {
      const result = spawnSync(binary, ["space argument", 'a"quoted"value', "研究 ü"], { cwd: user, input: "stdin text\n", encoding: "utf8", windowsHide: true });
      assert.ifError(result.error); assert.equal(result.status, 23);
      assert.equal(result.stdout.replaceAll("\r\n", "\n"), 'arg=--pipeline-cli\narg=space argument\narg=a"quoted"value\narg=研究 ü\nstdin text\n');
      assert.equal(result.stderr.trim(), "expected stderr");
    }
    fs.rmSync(path.join(user, "pipeline-cli-app"));
    const missing = spawnSync(installed, ["--version"], { encoding: "utf8", windowsHide: true });
    assert.equal(missing.status, 1); assert.match(missing.stderr, /Repair/);
    const info = spawnSync(installed, ["--pipeline-launcher-info"], { encoding: "utf8", windowsHide: true });
    assert.equal(info.status, 0); assert.equal(info.stdout.trim(), "pipeline-cli-launcher-v1");
    if (process.platform === "win32") {
      const python = resolvePython();
      const signal = spawnSync(python.command, [...python.args, path.join(root, "scripts/release/test-cli-launcher-control.py"), bundled], {
        encoding: "utf8", windowsHide: true, timeout: 60_000,
      });
      assert.ifError(signal.error); assert.equal(signal.status, 0, signal.stdout + signal.stderr);
    }
    const large = path.join(temporary, "large.exe"); fs.writeFileSync(large, Buffer.alloc(2_000_001));
    assert.throws(() => verifyCliLauncher(large, "1.0.0"), /full CLI/);
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
});
