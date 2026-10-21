import { spawnSync } from "node:child_process";

/** PYTHON is an executable path, never a shell command. */
export function resolvePython({ env = process.env, platform = process.platform, run = spawnSync } = {}) {
  const candidates = env.PYTHON
    ? [{ command: env.PYTHON, args: [] }]
    : [
        { command: "python3", args: [] },
        ...(platform === "win32" ? [{ command: "py", args: ["-3"] }] : []),
        { command: "python", args: [] },
      ];
  const failures = [];
  for (const candidate of candidates) {
    const result = run(candidate.command, [...candidate.args, "-c", "import sys; print('.'.join(map(str, sys.version_info[:3])))"], {
      env, encoding: "utf8", timeout: 10_000, windowsHide: true,
    });
    const version = /^(\d+)\.(\d+)\.(\d+)$/.exec(result.stdout?.trim() ?? "");
    if (!result.error && result.status === 0 && version && Number(version[1]) === 3 && Number(version[2]) >= 11)
      return candidate;
    failures.push(`${candidate.command}: ${result.error?.message ?? (result.stdout || result.stderr || `exit ${result.status}`).trim()}`);
  }
  throw new Error(`Release tests require Python 3.11 or later. Install Python 3 or set PYTHON to its executable path. Tried ${failures.join("; ")}`);
}
