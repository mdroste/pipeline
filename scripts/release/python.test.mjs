import assert from "node:assert/strict";
import test from "node:test";
import { resolvePython } from "./python.mjs";

const version = (stdout = "3.12.13\n") => ({ status: 0, stdout, stderr: "" });
const missing = () => ({ status: null, error: Object.assign(new Error("spawn ENOENT"), { code: "ENOENT" }) });

test("release launcher works with only python3 on PATH", () => {
  const commands = [];
  const result = resolvePython({ env: { PATH: "/tools" }, platform: "darwin", run(command, _args, options) {
    commands.push(command);
    assert.equal(options.env.PATH, "/tools");
    return command === "python3" ? version() : missing();
  } });
  assert.deepEqual(result, { command: "python3", args: [] });
  assert.deepEqual(commands, ["python3"]);
});

test("explicit Python paths retain spaces and are not split into shell arguments", () => {
  const python = "C:\\Program Files\\Python α\\python.exe";
  const result = resolvePython({ env: { PYTHON: python }, run(command, args) {
    assert.equal(command, python);
    assert.equal(args[0], "-c");
    return version();
  } });
  assert.equal(result.command, python);
});

test("Windows launcher retains -3 and skips unsupported Python versions", () => {
  const result = resolvePython({ env: {}, platform: "win32", run(command, args) {
    if (command === "python3") return version("3.9.9\n");
    assert.equal(command, "py");
    assert.equal(args[0], "-3");
    return version();
  } });
  assert.deepEqual(result, { command: "py", args: ["-3"] });
});

test("missing explicit interpreter gives an actionable spawn error without silently falling back", () => {
  let calls = 0;
  assert.throws(() => resolvePython({ env: { PYTHON: "/missing/python" }, run() { calls++; return missing(); } }), /Python 3\.11.*\/missing\/python: spawn ENOENT/);
  assert.equal(calls, 1);
});
