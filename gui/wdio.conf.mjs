import path from "node:path";
import process from "node:process";
import os from "node:os";

const executable = process.platform === "win32" ? "pipeline-gui.exe" : "pipeline-gui";
const appBinaryPath = path.resolve("src-tauri", "target", "debug", executable);

export const config = {
  runner: "local",
  specs: ["./e2e/**/*.spec.mjs"],
  maxInstances: 1,
  capabilities: [{ browserName: "tauri" }],
  services: [[
    "@wdio/tauri-service",
    {
      appBinaryPath,
      driverProvider: "embedded",
      startTimeout: 60_000,
      commandTimeout: 30_000,
      captureBackendLogs: true,
      captureFrontendLogs: true,
      logDir: path.join(os.tmpdir(), "pipeline-wdio-logs"),
    },
  ]],
  framework: "mocha",
  reporters: ["spec"],
  // Service 1.2.0 reports a missing external tauri-driver even when the
  // configured embedded provider is healthy; keep that false diagnostic out
  // of otherwise successful CI logs. The spec reporter still prints failures.
  logLevel: "silent",
  mochaOpts: {
    timeout: 60_000,
  },
};
