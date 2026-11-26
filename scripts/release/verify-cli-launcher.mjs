import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export function verifyCliLauncher(binary, version) {
  if (fs.statSync(binary).size > 2_000_000) throw new Error("Desktop package contains the full CLI instead of the small launcher");
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline CLI ü qualification "));
  const run = (args, input) => {
    const result = spawnSync(binary, args, { cwd, input, encoding: "utf8", timeout: 30_000, windowsHide: true });
    if (result.error) throw result.error;
    return result;
  };
  try {
    const info = run(["--pipeline-launcher-info"]);
    if (info.status !== 0 || info.stdout.trim() !== "pipeline-cli-launcher-v1") throw new Error("Not a Pipeline compatibility launcher");
    const found = run(["--version"]);
    if (found.status !== 0 || found.stdout.trim() !== `pipeline-cli ${version}`) throw new Error(`CLI version forwarding failed: ${found.stdout} ${found.stderr}`);
    const invalid = run(["--invalid-command"]);
    if (invalid.status !== 2 || !invalid.stderr.includes("error:")) throw new Error("CLI exit code/stderr forwarding failed");
    const template = run(["workflow", "template"]);
    if (template.status !== 0) throw new Error(`CLI workflow template failed: ${template.stderr}`);
    const validation = run(["workflow", "validate", "-", "--json"], template.stdout);
    if (validation.status !== 0) throw new Error(`CLI stdin forwarding failed: ${validation.stderr}`);
    JSON.parse(validation.stdout);
    console.log("CLI launcher passed: version, headless operation, stdin/stdout/stderr, exit code, unrelated working directory");
  } finally { fs.rmSync(cwd, { recursive: true, force: true }); }
}

// Check the installed-user path too: its launcher points to the outer AppImage,
// never to an ephemeral mount that disappears when the desktop closes.
export function verifyAppImageCli(appimage, version) {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline AppImage CLI "));
  try {
    const extract = spawnSync(appimage, ["--appimage-extract", "usr/bin/pipeline-cli"], {
      cwd: temporary, encoding: "utf8", timeout: 60_000,
    });
    if (extract.error || extract.status !== 0) throw new Error(`Cannot extract AppImage launcher: ${extract.error || extract.stderr}`);
    const launcher = path.join(temporary, "pipeline-cli");
    fs.copyFileSync(path.join(temporary, "squashfs-root/usr/bin/pipeline-cli"), launcher);
    fs.chmodSync(launcher, 0o755);
    fs.writeFileSync(path.join(temporary, "pipeline-cli-app"), appimage);
    verifyCliLauncher(launcher, version);
  } finally { fs.rmSync(temporary, { recursive: true, force: true }); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const version = JSON.parse(fs.readFileSync(new URL("../../gui/package.json", import.meta.url))).version;
  if (process.argv[2] === "--appimage") verifyAppImageCli(path.resolve(process.argv[3]), version);
  else verifyCliLauncher(path.resolve(process.argv[2]), version);
}
