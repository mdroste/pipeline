import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT = fileURLToPath(new URL("./validate-macos-machos.sh", import.meta.url));

// Stand-ins for the macOS tools, so this also runs on the Linux test job.
const TOOLS = {
  file: 'echo "$1: Mach-O 64-bit executable arm64"\n',
  lipo: "echo arm64\n",
  otool: [
    'case "$1" in',
    "  -l) printf 'Load command 1\\n      cmd LC_BUILD_VERSION\\n    minos 15.0\\n' ;;",
    "  -D) echo \"$2:\" ;;",
    "  -L) printf '%s:\\n\\t/usr/lib/libSystem.B.dylib (compatibility version 1.0.0)\\n' \"$2\" ;;",
    "esac",
    "",
  ].join("\n"),
};

function runValidator(codesign) {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline-machos-"));
  try {
    const bin = path.join(folder, "bin");
    const root = path.join(folder, "root");
    fs.mkdirSync(bin);
    fs.mkdirSync(root);
    fs.writeFileSync(path.join(root, "pipeline-gui"), "binary");
    for (const [name, body] of Object.entries({ ...TOOLS, codesign })) {
      fs.writeFileSync(path.join(bin, name), `#!/bin/bash\n${body}`, { mode: 0o755 });
    }
    return spawnSync("bash", [
      SCRIPT, "--root", root, "--expected-arch", "arm64", "--max-minos", "15.0",
      "--require-developer-id",
    ], {
      encoding: "utf8",
      env: { ...process.env, PATH: `${bin}${path.delimiter}${process.env.PATH}` },
    });
  } finally {
    fs.rmSync(folder, { recursive: true, force: true });
  }
}

const skip = process.platform === "win32" ? "needs bash" : false;

// codesign prints Authority partway through, then keeps writing to stderr. A
// reader that stops at the first match used to fail every signed binary.
test("accepts a Developer ID signature followed by more output", { skip }, () => {
  const result = runValidator([
    '[ "$1" = "--verify" ] && exit 0',
    "echo 'Executable=/app' >&2",
    "echo 'Authority=Developer ID Application: Example (TEAM)' >&2",
    "sleep 0.2",
    "for i in 1 2 3 4 5; do echo 'Timestamp=later' >&2; done",
    "",
  ].join("\n"));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Validated 1 Mach-O files/);
});

test("rejects a binary without a Developer ID signature", { skip }, () => {
  const result = runValidator('[ "$1" = "--verify" ] && exit 0\necho "Signature=adhoc" >&2\n');
  assert.equal(result.status, 1);
  assert.match(result.stderr, /lacks a Developer ID Application signature/);
});
