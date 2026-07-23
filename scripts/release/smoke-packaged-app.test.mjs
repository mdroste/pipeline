import assert from "node:assert/strict";
import process from "node:process";
import test from "node:test";

import { smokePackagedApp } from "./smoke-packaged-app.mjs";

test("packaged-app smoke waits for the frontend readiness marker", async () => {
  const script = [
    "require('node:fs').writeFileSync(process.env.PIPELINE_SMOKE_READY_FILE, 'ready\\n');",
    "setInterval(() => {}, 1000);",
  ].join("");
  await assert.doesNotReject(
    smokePackagedApp(process.execPath, ["-e", script], { timeoutMs: 5_000 }),
  );
});

test("packaged-app smoke fails when the process exits before readiness", async () => {
  await assert.rejects(
    smokePackagedApp(process.execPath, ["-e", "process.exit(7)"], { timeoutMs: 5_000 }),
    /exited before becoming ready \(code=7/,
  );
});
