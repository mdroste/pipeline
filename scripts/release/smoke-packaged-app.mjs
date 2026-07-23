#!/usr/bin/env node
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const MAX_LOG_BYTES = 64 * 1024;

function appendLog(current, chunk) {
  const next = current + chunk.toString();
  return next.length > MAX_LOG_BYTES ? next.slice(-MAX_LOG_BYTES) : next;
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

export async function smokePackagedApp(executable, args = [], options = {}) {
  const timeoutMs = options.timeoutMs ?? 30_000;
  const marker = path.join(
    os.tmpdir(),
    `pipeline-smoke-${process.pid}-${Date.now()}-${Math.random().toString(16).slice(2)}.ready`,
  );
  let stdout = "";
  let stderr = "";
  let exited = false;
  let exitCode = null;
  let exitSignal = null;
  let spawnError = null;

  const child = spawn(executable, args, {
    env: { ...process.env, PIPELINE_SMOKE_READY_FILE: marker },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  child.stdout.on("data", (chunk) => { stdout = appendLog(stdout, chunk); });
  child.stderr.on("data", (chunk) => { stderr = appendLog(stderr, chunk); });
  child.once("error", (error) => {
    spawnError = error;
    exited = true;
  });
  child.once("exit", (code, signal) => {
    exited = true;
    exitCode = code;
    exitSignal = signal;
  });

  const deadline = Date.now() + timeoutMs;
  try {
    while (!fs.existsSync(marker)) {
      if (spawnError) throw new Error(`failed to launch application: ${spawnError.message}`);
      if (exited) {
        throw new Error(
          `application exited before becoming ready (code=${exitCode}, signal=${exitSignal})\n${stdout}${stderr}`,
        );
      }
      if (Date.now() >= deadline) {
        throw new Error(`application did not become ready within ${timeoutMs}ms\n${stdout}${stderr}`);
      }
      await delay(100);
    }
  } finally {
    if (!exited) child.kill();
    const stopDeadline = Date.now() + 5_000;
    while (!exited && Date.now() < stopDeadline) await delay(50);
    if (!exited) child.kill("SIGKILL");
    fs.rmSync(marker, { force: true });
  }

  return { stdout, stderr };
}

const invokedDirectly = process.argv[1]
  && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedDirectly) {
  const [, , executable, ...args] = process.argv;
  if (!executable) {
    console.error("usage: smoke-packaged-app.mjs <executable> [arguments...]");
    process.exit(2);
  }
  await smokePackagedApp(executable, args);
  console.log(`Packaged application reached frontend readiness: ${executable}`);
}
