// Development fixture for the run console. `?theme=dark` and `?width=640`
// preview the console in a dark or narrow window.
import { createRoot } from "react-dom/client";
import Console from "../src/components/Console";
import type {
  LlmRequestDetails,
  LogEntry,
  TokenTotals,
  UsageState,
} from "../src/lib/reviewRuntimeTypes";
import "../src/App.css";

const params = new URLSearchParams(window.location.search);
if (params.get("theme") === "dark")
  document.documentElement.classList.add("dark");
const width = Number(params.get("width")) || undefined;

const base: LlmRequestDetails = {
  provider: "claude",
  provider_label: "Claude Code",
  transport: "cli",
  model: "claude-opus-4-6",
  model_policy: "Best available",
  effort: "high",
  tools: ["Read", "Grep", "Glob"],
  timeout_secs: 1800,
  max_output_tokens: null,
  output_format: "text",
  prompt:
    "You are reviewing the attached economics paper for technical correctness.\n\nCheck every derivation in Sections 2–4 and report each issue with a location, a severity, and a suggested fix.",
  prompt_chars: 18_402,
  system_prompt: "Respond in Markdown. Cite page numbers for every claim.",
  system_prompt_chars: 61,
  shared_context: "# Paper survey\n\nThe paper studies production networks…",
  shared_context_chars: 42_118,
  pdf_attached: false,
  write_enabled: false,
  working_directory: "/Users/me/Papers/production-networks",
  read_directories: [
    "/Users/me/Papers/production-networks",
    "/Users/me/Library/Application Support/pipeline/runs/2026-10-26/artifacts",
  ],
  write_directory: null,
  local_endpoint: null,
};

const codex: LlmRequestDetails = {
  ...base,
  provider: "codex",
  provider_label: "Codex",
  model: "gpt-5.6",
  model_policy: "Pinned",
  effort: "xhigh",
  tools: ["shell"],
};

const gemini: LlmRequestDetails = {
  ...base,
  provider: "antigravity",
  provider_label: "Google",
  transport: "api",
  model: "gemini-3-pro",
  model_policy: "Balanced",
  effort: "medium",
  tools: [],
  timeout_secs: 600,
  max_output_tokens: 64_000,
  pdf_attached: true,
  shared_context: null,
  shared_context_chars: 0,
  write_enabled: true,
  write_directory:
    "/Users/me/Library/Application Support/pipeline/runs/2026-10-26/extraction",
};

const sessions: Array<[number, string, LlmRequestDetails, string[], boolean?]> =
  [
    [
      1,
      "LLM PDF extraction",
      gemini,
      ["Uploading PDF (2.4 MB)", "Wrote extraction/paper.md"],
    ],
    [2, "Step: Orientation", base, ["Reading survey", "Wrote orientation.md"]],
    [
      3,
      "Step: Technical review [Claude]",
      base,
      ["Checking Section 2 derivations", "Checking Section 3 derivations"],
    ],
    [
      4,
      "Step: Technical review [Codex]",
      codex,
      ["$ rg -n 'Proposition' paper.md", "Found 14 matches"],
    ],
    [5, "Step: Writing review [Section 2 · Claude]", base, ["Reviewing prose"]],
    [6, "Merge: Technical review", base, ["Merging 2 reviews"]],
    [
      7,
      "Step: Synthesis · usage-limit fallback",
      codex,
      ["ERROR: Codex usage limit reached; retry after 14:05"],
      true,
    ],
  ];

const logs: LogEntry[] = [
  {
    line: "Starting run · 6 steps · 3 providers",
    session: null,
    label: null,
    level: "info",
    t: Date.now() - 60_000,
  },
];
for (const [id, label, request, lines, failed] of sessions) {
  logs.push({
    line: `LLM request · ${request.provider_label}`,
    session: id,
    label,
    level: "info",
    t: Date.now() - 50_000 + id * 1000,
    request,
  });
  for (const line of lines) {
    logs.push({
      line,
      session: id,
      label,
      level: line.startsWith("ERROR")
        ? "error"
        : line.startsWith("$")
          ? "stdout"
          : "info",
      t: Date.now() - 40_000 + id * 1000,
    });
  }
  if (!failed)
    logs.push({
      line: "WARNING: response exceeded soft length budget",
      session: id,
      label,
      level: "warn",
      t: Date.now(),
    });
}

const noTools = {
  text_file: 0,
  image: 0,
  web: 0,
  shell_or_other: 0,
  unknown: 0,
};
const totals = (
  input: number,
  cached: number,
  cacheWrite: number,
  output: number,
): TokenTotals => ({
  input,
  cached,
  cacheWrite,
  output,
  modelRoundTrips: 0,
  toolCalls: noTools,
});
const bySession: Record<number, TokenTotals> = {
  1: totals(184_220, 0, 0, 41_870),
  2: totals(52_400, 38_100, 9_200, 3_480),
  3: totals(412_900, 361_000, 22_000, 18_250),
  4: totals(288_100, 240_300, 0, 12_910),
  5: totals(61_700, 44_000, 8_100, 5_020),
  6: totals(33_000, 0, 0, 6_400),
};
const sum = Object.values(bySession).reduce(
  (acc, u) =>
    totals(
      acc.input + u.input,
      acc.cached + u.cached,
      acc.cacheWrite + u.cacheWrite,
      acc.output + u.output,
    ),
  totals(0, 0, 0, 0),
);
const usage: UsageState = { total: sum, bySession };

createRoot(document.getElementById("root")!).render(
  <div
    className="flex h-screen flex-col bg-white dark:bg-gray-950"
    style={{ width }}
  >
    <div className="flex-1" />
    <Console logs={logs} usage={usage} active />
  </div>,
);
