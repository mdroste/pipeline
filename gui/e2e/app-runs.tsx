// Development-only fixture: the full app (app-full-shim.ts) plus a live review
// run queue. Two reviews run, two wait, one finished, one failed; running
// reviews receive synthetic `pipeline:*` events so progress, console, and the
// sidebar list behave as they do against the real backend. Queue commands
// (enqueue, cancel, dismiss) mutate the fixture queue and publish
// `runs:queue`. Open /e2e/app-runs.html?page=home (theme=dark supported).
import "./app-full-shim";
import { createRoot } from "react-dom/client";
import App from "../src/App";
import ErrorBoundary from "../src/components/ErrorBoundary";
import DialogService from "../src/components/DialogService";
import type { QueuedRun } from "../src/lib/types";
import { makeSettings } from "../src/test/settingsFixtures";
import "katex/dist/katex.min.css";
import "../src/App.css";

type Callback = (event: {
  event: string;
  id: number;
  payload: unknown;
}) => void;
const internals = (window as any).__TAURI_INTERNALS__;
const baseInvoke = internals.invoke as (
  command: string,
  args?: any,
) => Promise<unknown>;
const callbacks = new Map<number, Callback>();
const listeners = new Map<string, Set<number>>();
let nextCallback = 50_000;
internals.transformCallback = (callback: Callback) => {
  const id = nextCallback++;
  callbacks.set(id, callback);
  return id;
};
(window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (event: string, id: number) =>
    listeners.get(event)?.delete(id),
};
function emit(event: string, payload: unknown) {
  for (const id of listeners.get(event) ?? [])
    callbacks.get(id)?.({ event, id, payload });
}

const now = Date.now();
const minutes = (count: number) => count * 60_000;
function run(
  key: string,
  label: string,
  status: QueuedRun["status"],
  extra: Partial<QueuedRun> = {},
): QueuedRun {
  return {
    key,
    kind: "review",
    label,
    inputPath: `/Users/mike/Papers/${label}`,
    profileName: "Auto Paper Review",
    status,
    stage: null,
    stageLabel: null,
    waiting: null,
    queuedAt: now - minutes(20),
    startedAt: null,
    finishedAt: null,
    runId: null,
    error: null,
    partial: false,
    resultAvailable: false,
    ...extra,
  };
}
const queue: { limit: number; runs: QueuedRun[] } = {
  limit: 2,
  runs: [
    run("run-health", "health-insurance-rct.pdf", "done", {
      startedAt: now - minutes(48),
      finishedAt: now - minutes(6),
      resultAvailable: true,
      runId: "run-2026-09-28-a",
    }),
    run("run-fiscal", "fiscal-multipliers.docx", "failed", {
      startedAt: now - minutes(40),
      finishedAt: now - minutes(31),
      error:
        "Claude CLI exited with status 1: usage limit reached for this billing period.",
    }),
    run("run-monetary", "monetary-policy-draft.pdf", "running", {
      startedAt: now - minutes(14) - 12_000,
      stage: "dispatching",
      stageLabel: "Parallel review",
    }),
    run("run-trade", "trade-elasticities.tex", "running", {
      startedAt: now - 64_000,
      stage: "extracting",
      stageLabel: "Extract document",
    }),
    run("run-labor", "labor-search-appendix.pdf", "queued", {
      queuedAt: now - minutes(3),
    }),
    run("run-climate", "climate-risk-pricing.pdf", "queued", {
      queuedAt: now - minutes(1),
    }),
  ],
};
const publish = () => emit("runs:queue", structuredClone(queue));
const finished = (status: QueuedRun["status"]) =>
  !["queued", "running"].includes(status);
let enqueued = 0;
function enqueue(paperPath: string): QueuedRun {
  const label = paperPath.split("/").pop() || "Review";
  const entry = run(`run-new-${++enqueued}`, label, "queued", {
    queuedAt: Date.now(),
    inputPath: paperPath,
  });
  queue.runs.push(entry);
  setTimeout(publish, 0);
  return entry;
}

const result = {
  run_id: "run-2026-09-28-a",
  markdown:
    "# Referee report\n\nThe paper estimates the effect of a randomized insurance offer on utilization.\n\n## Major comments\n\n1. The attrition analysis needs bounds.\n2. The pre-registered outcomes differ from the reported ones.\n",
  extracted_text: "Health insurance RCT draft",
  report: {
    orientation: null,
    step_outputs: [],
    failed_steps: [],
    report_date: "2026-10-26",
    paper_hash: "a1b2c3d4e5f60718",
  },
};

internals.invoke = async (command: string, args?: any) => {
  switch (command) {
    case "plugin:event|listen": {
      const set = listeners.get(args.event) ?? new Set<number>();
      set.add(args.handler);
      listeners.set(args.event, set);
      return args.handler;
    }
    case "plugin:event|unlisten":
      listeners.get(args.event)?.delete(args.eventId);
      return null;
    case "list_run_queue":
      return structuredClone(queue);
    case "get_queued_run_result":
      return args.key === "run-health" ? structuredClone(result) : null;
    case "enqueue_run":
      return enqueue(args.paperPath);
    case "enqueue_batch":
      return (args.paths as string[]).map(enqueue);
    case "rerun_requirements":
      return { secrets: [], notice: null };
    case "enqueue_rerun":
      return enqueue(`Re-run · ${args.runId}`);
    case "cancel_pipeline": {
      const entry = queue.runs.find(
        (candidate) => candidate.key === args.runKey,
      );
      if (entry && !finished(entry.status)) {
        if (entry.status === "running") entry.error = "Pipeline cancelled";
        entry.status = "cancelled";
        entry.finishedAt = Date.now();
        publish();
      }
      return null;
    }
    case "dismiss_queued_runs":
      queue.runs = queue.runs.filter(
        (entry) =>
          !finished(entry.status) || (args.key && args.key !== entry.key),
      );
      publish();
      return null;
    case "cancel_pass":
      return null;
    // The shared shim returns bare settings; Settings needs the envelope.
    case "get_settings":
      return {
        settings: {
          ...makeSettings(),
          ...((await baseInvoke(command, args)) as object),
          max_concurrent_reviews: queue.limit,
        },
        warnings: [],
      };
    default:
      return baseInvoke(command, args);
  }
};

// Replay the running reviews' history once the app has subscribed.
function replay() {
  if (!listeners.get("pipeline:stage")?.size) {
    setTimeout(replay, 100);
    return;
  }
  const runKey = "run-monetary";
  const stage = (
    stageKind: string,
    id: string,
    label: string,
    stepIds: string[] = [],
    stepLabels: string[] = [],
  ) =>
    emit("pipeline:stage", {
      stage: stageKind,
      id,
      label,
      stepIds,
      stepLabels,
      runKey,
    });
  const pass = (name: string, status: string, key = runKey) =>
    emit("pipeline:pass", { name, status, runKey: key });
  const log = (
    line: string,
    session: number | null = null,
    label: string | null = null,
    key = runKey,
  ) =>
    emit("pipeline:log", {
      line,
      session,
      label,
      level: line.startsWith("WARNING") ? "warn" : "info",
      runKey: key,
    });
  stage("extracting", "extracting", "Extract document");
  log("Extracting document from /Users/mike/Papers/monetary-policy-draft.pdf");
  log("Extracted via paddleocr-vl-full (184,220 chars, 212s)");
  stage("orienting", "orientation", "Orientation map");
  log("Orientation map ready", 1, "Orientation map");
  emit("pipeline:routing", {
    primaryDomain: "Economics",
    subject: "Monetary economics",
    specialistIds: ["subject_economics_monetary"],
    specialistLabels: ["Economics — Monetary"],
    runKey,
  });
  stage(
    "dispatching",
    "wave-1",
    "Parallel review",
    ["first_pass", "methods_pass"],
    ["First reading", "Methods deep dive"],
  );
  for (const name of [
    "first_pass/claude",
    "first_pass/codex",
    "methods_pass/claude",
    "methods_pass/codex",
  ])
    pass(name, "running");
  pass("first_pass/codex", "done");
  log("First reading: 11 findings drafted", 2, "First reading · codex");
  log("Reading section 4 (identification)…", 3, "Methods deep dive · claude");
  emit("pipeline:usage", {
    session: 2,
    input_tokens: 182_000,
    output_tokens: 14_400,
    cached_input_tokens: 96_000,
    runKey,
  });
  emit("pipeline:stage", {
    stage: "extracting",
    id: "extracting",
    label: "Extract document",
    stepIds: [],
    runKey: "run-trade",
  });
  log(
    "Extracting document from /Users/mike/Papers/trade-elasticities.tex",
    null,
    null,
    "run-trade",
  );
  // Keep the page alive: passes finish one by one.
  const pending = ["first_pass/claude", "methods_pass/codex"];
  const timer = setInterval(() => {
    const next = pending.shift();
    if (!next) return clearInterval(timer);
    pass(next, "done");
    log(`${next} finished`, null, null);
  }, 6000);
}
replay();

createRoot(document.getElementById("root")!).render(
  <ErrorBoundary>
    <DialogService>
      <App />
    </DialogService>
  </ErrorBoundary>,
);
