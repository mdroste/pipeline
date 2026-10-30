import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";
import yaml from "../../gui/node_modules/js-yaml/index.js";
import { windowsSigningConfig } from "./prepare-windows-signing.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("Windows signing preserves checkout paths with spaces as single arguments", () => {
  const config = windowsSigningConfig(path.join(os.tmpdir(), "a checkout with spaces"));
  const { cmd, args } = config.bundle.windows.signCommand;
  assert.equal(cmd, "pwsh");
  assert.equal(args[2], path.join(os.tmpdir(), "a checkout with spaces/scripts/release/windows-sign.ps1"));
  assert.deepEqual(args.slice(3), ["-FilePath", "%1"]);
  assert.deepEqual(Object.keys(config), ["bundle"]);
});

test("release verifies installed Windows signatures before upload and cannot use unsigned fallback", () => {
  const workflow = yaml.load(fs.readFileSync(path.join(root, ".github/workflows/release.yml"), "utf8"));
  const job = workflow.jobs.package;
  assert.equal(job.permissions["id-token"], "write");
  assert.equal(job.environment.name, "${{ matrix.id == 'windows' && 'windows-signing' || 'release' }}");
  const steps = job.steps;
  const index = (name) => {
    const found = steps.findIndex((step) => step.name === name);
    assert.notEqual(found, -1, name);
    return found;
  };
  const auth = index("Authenticate Windows signing to Azure");
  const compile = index("Compile Windows application before signing login");
  const prepare = index("Prepare Windows signing and sign bundled Poppler");
  const build = index("Build signed Windows package");
  const verify = index("Verify Windows installer and installed signatures");
  assert.ok(compile < auth && auth < prepare && prepare < build && build < verify && verify < index("Upload immutable installer to draft"));
  assert.match(steps[compile].run, /build --no-bundle/);
  assert.match(steps[build].run, /tauri -- bundle --config/);
  for (const i of [compile, auth, prepare, build, verify]) {
    assert.equal(steps[i].if, "matrix.id == 'windows'");
    assert.ok(!steps[i]["continue-on-error"]);
  }
  assert.equal(steps[index("Build unsigned package")].if, "matrix.id == 'linux'");
});

const powershell = process.env.PIPELINE_TEST_PWSH || "pwsh";
const available = spawnSync(powershell, ["-NoProfile", "-Command", "exit 0"]).status === 0;
test("Windows installer paths and signature rejection gates preserve their boundary contracts", {
  skip: !available && process.platform !== "win32" ? "PowerShell is not installed" : false,
}, () => {
  assert.ok(available, "Windows release tests require PowerShell");
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "pipeline signing tests "));
  try {
    const target = path.join(temporary, "test binary.exe");
    fs.writeFileSync(target, "fixture");
    const result = spawnSync(powershell, ["-NoProfile", "-File", path.join(root, "scripts/release/windows-signing.test.ps1"), "-Target", target], {
      encoding: "utf8", timeout: 60000,
    });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});
