import type { PipelineReport, ToolCallCounts } from "./types";

export type PassStatus = "pending" | "running" | "done" | "error" | "skipped";

/** Effective settings and prompt material for one provider invocation. Secrets
 *  are never included. Large prompt fields arrive only in the live event and
 *  are not written to the persistent console transcript. */
export interface LlmRequestDetails {
  provider: string;
  provider_label: string;
  transport: "cli" | "api";
  model: string;
  model_policy: string;
  effort: string;
  tools: string[];
  timeout_secs: number;
  max_output_tokens: number | null;
  output_format: string;
  prompt: string;
  prompt_chars: number;
  prompt_truncated?: boolean;
  system_prompt: string | null;
  system_prompt_chars?: number;
  system_prompt_truncated?: boolean;
  shared_context: string | null;
  shared_context_chars: number;
  shared_context_truncated?: boolean;
  pdf_attached: boolean;
  write_enabled: boolean;
  working_directory: string | null;
  read_directories: string[];
  write_directory: string | null;
  local_endpoint: string | null;
}

/** One console line, tagged with the headless session (LLM call) it came from.
 *  `session` is null for orchestration/extraction lines that belong to no
 *  single call; those show only in the master view. */
export interface LogEntry {
  line: string;
  session: number | null;
  label: string | null;
  level: string; // info | warn | error | stderr | stdout
  /** Client arrival time (ms epoch), used for optional console timestamps. */
  t: number;
  /** Present on the first line for an LLM session. */
  request?: LlmRequestDetails;
}

/** Start/end times (ms epoch) of a single pass, for elapsed-time display. */
export interface PassTiming {
  start: number;
  end?: number;
}

export type RuntimeStageKind =
  | "extracting"
  | "orienting"
  | "dispatching"
  | "merging"
  | "synthesizing"
  | "done";

export interface RuntimeStage {
  /** Stable ID emitted by the backend and shared with get_execution_plan. */
  id: string;
  kind: RuntimeStageKind;
  label: string;
  stepIds: string[];
  stepLabels?: string[];
  /** Multi-provider steps that will receive a merge after this dispatch wave. */
  mergeStepIds?: string[];
  mergeStepLabels?: string[];
  passes: Record<string, PassStatus>;
  status: "active" | "done" | "failed" | "skipped";
}

export interface ReviewRoutingSummary {
  primaryDomain: string;
  subject: string;
  subjectIds?: string[];
  subjectLabels?: string[];
  methodIds?: string[];
  methodLabels?: string[];
  specialistIds: string[];
  specialistLabels: string[];
}

export interface ProviderLimitNotice {
  pass_key: string;
  label: string;
  provider: string;
  message: string;
  status: "fallback_starting" | "recovered" | "fallback_failed" | "exhausted";
  fallback_provider: string | null;
  fallback_model: string | null;
}

export interface TokenTotals {
  input: number;
  output: number;
  cached: number;
  cacheWrite: number;
  modelRoundTrips: number;
  toolCalls: ToolCallCounts;
}

/** Token usage aggregated over a run: a grand total plus per-session counts.
 *  Every current provider reports usage — Codex, Claude CLI JSON,
 *  and the direct APIs. */
export interface UsageState {
  total: TokenTotals;
  bySession: Record<number, TokenTotals>;
}

export type PipelineState =
  | { kind: "idle" }
  | { kind: "extracting" }
  | { kind: "orienting" }
  | { kind: "dispatching"; passes: Record<string, PassStatus> }
  | { kind: "merging"; passes: Record<string, PassStatus> }
  | { kind: "synthesizing"; passes: Record<string, PassStatus> }
  | {
      kind: "done";
      markdown: string;
      report: PipelineReport;
      extractedText: string;
      runId: string | null;
    }
  | { kind: "error"; message: string; failedAt?: string };
