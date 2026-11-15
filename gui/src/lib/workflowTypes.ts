import type { ModelSelection } from "./reviewModelTypes";
import type { Settings } from "./settingsTypes";

export type Phase = "parallel" | "sequential";

/** Deterministic guard controlling whether a step runs (mirrors RunCondition). */
export type RunCondition =
  | { kind: "output_matches"; step: string; pattern: string; negate?: boolean }
  | {
      kind: "survey_path";
      pointer: string;
      equals?: unknown;
      exists?: boolean;
      contains?: unknown;
    };

export type PrimaryArtifactPart = "text" | "structure" | "visuals" | "source";
export type NamedInputArtifactPart = "text" | "source";
export type StepArtifactPart = "report" | "files";

/** A logical artifact selection. Paths are resolved privately for each call. */
export type ArtifactSelector =
  | { kind: "primary"; parts: PrimaryArtifactPart[] }
  | { kind: "survey" }
  | { kind: "named_input"; key: string; parts: NamedInputArtifactPart[] }
  | { kind: "step"; step: string; parts: StepArtifactPart[]; glob?: string };

/** Exact artifact allowlist for a step. Empty means intentionally isolated. */
export interface StepContext {
  include: ArtifactSelector[];
}

export interface StepConfig {
  id: string;
  label: string;
  /** "analyzer" steps run a Pipeline check and make no model call. */
  kind?: "model" | "analyzer";
  /** Required for analyzer steps. */
  analyzer?: AnalyzerSpec | null;
  prompt: string;
  /** Literal trusted reviewer instructions; no template expansion. */
  system_prompt?: string;
  enabled: boolean;
  phase: Phase;
  tools: string[];
  agents: string[];
  /** Per-step model override; empty/undefined = use the global setting. */
  model?: string;
  /** Provider/transport-specific model policies (for example codex:cli). */
  model_overrides?: Record<string, ModelSelection>;
  /** Per-step effort override; empty/undefined = use the global setting. */
  effort?: string;
  effort_overrides?: Record<string, string>;
  /** Order-only dependencies. Upstream artifact selectors add dataflow edges automatically. */
  after?: string[];
  /** Success requirements. Every step named here is also a scheduling edge. */
  dependency_policy?: DependencyPolicy;
  /** Exact artifacts available to this step. */
  context: StepContext;
  /** Optional guard: skip the step unless this condition holds. */
  run_if?: RunCondition | null;
  /** Optional JSON-shape contract the step's output must satisfy (with retry). */
  output_schema?: Record<string, unknown> | null;
  /** Fan-out: run once per matching file or upstream JSON-array item, with {item} bound. */
  for_each?: ForEach | null;
}

/** What must succeed before a step runs (mirrors DependencyPolicy). */
export interface DependencyPolicy {
  required?: string[];
  quorum?: string[];
  /** How many `quorum` members must succeed; zero when the quorum is empty. */
  minimum_successes?: number;
}

/** A pinned host analyzer and its options (mirrors AnalyzerSpec). */
export interface AnalyzerSpec {
  id: string;
  version: number;
  options?: Record<string, unknown>;
}

/** Fan-out (map) config for a step (mirrors ForEach). */
export interface ForEach {
  glob: string;
  max: number;
  /** Fan out over an upstream artifact's array elements instead of files. */
  artifact?: ForEachArtifact | null;
}

/** Upstream array source for artifact fan-out (mirrors ForEachArtifact). */
export interface ForEachArtifact {
  step: string;
  pointer?: string;
}

export interface MergeConfig {
  enabled: boolean;
  prompt: string;
  agents: string[];
}

/** An additional named input a profile accepts (mirrors InputSlot). */
export interface InputSlot {
  key: string;
  label?: string;
  /** "document" | "folder". */
  mode?: string;
  required?: boolean;
  extensions?: string[];
  mime_types?: string[];
  max_bytes?: number;
  sensitivity?: "" | "public" | "internal" | "confidential" | "secret";
}

/** Run-time meaning assigned to the primary file-system selection. */
export type InputInterpretation =
  "document" | "latex_project" | "source_tree" | "batch";

/** One primary selection may contain several paths when it represents a batch. */
export interface PrimaryInputSelection {
  reviewHandoffId?: string;
  paths: string[];
  interpretation: InputInterpretation;
  selectionKind: "file" | "folder";
}

/** Per-profile extraction configuration. Parser tuning lives in global Settings. */
export interface ExtractionConfig {
  /** "" | "auto" | "llm" | "paddleocr-vl-full" | "pdftotext". */
  method: string;
  /** "" or "document" | "folder" | "none". Empty = document. */
  input_mode?: string;
  /** Extra named inputs that steps may select; selected text resolves as {input:key}. */
  extra_inputs?: InputSlot[];
}

/** A run-time variable a profile declares (mirrors VarSpec). */
export interface VarSpec {
  key: string;
  label?: string;
  /** "text" | "choice" | "file". */
  kind?: string;
  default?: string;
  choices?: string[];
  required?: boolean;
  secret?: boolean;
  validation?: {
    min_length?: number;
    max_length?: number;
    pattern?: string;
  };
}

/** Optional, profile-scoped reuse of each step's selected primary text/survey. */
export interface ContextCacheConfig {
  enabled: boolean;
}

export interface PipelineConfig {
  steps: StepConfig[];
  merge: MergeConfig;
  /** Explicit public products. Empty fields retain legacy final-step inference. */
  outputs?: OutputConfig;
  /** Missing on older profiles/backends and therefore treated as disabled. */
  context_cache?: ContextCacheConfig;
  /** Compatibility field; the backend and editor normalize this to true. */
  use_orientation: boolean;
  /** Custom orientation-map prompt. Empty = use the default (prompts/orientation.md). */
  orientation_prompt: string;
  /** Optional JSON-shape contract for the orientation call. */
  orientation_schema?: Record<string, unknown> | null;
  extraction: ExtractionConfig;
  parallel_context_template: string;
  /** Run-time variables the profile declares; referenced in prompts as {var:key}. */
  variables?: VarSpec[];
}

export interface OutputConfig {
  primary_step?: string;
  findings_step?: string;
  named?: NamedProductSpec[];
}

export interface NamedProductSpec {
  key: string;
  step: string;
  media_type?: string;
  viewer?: "" | "text" | "markdown" | "json" | "artifact";
  export_policy?: "" | "full" | "redacted" | "disabled";
  sensitivity?: "" | "public" | "internal" | "confidential" | "secret";
  schema?: Record<string, unknown> | null;
}

export interface ProfileSummary {
  id: string;
  name: string;
  step_count: number;
  builtin: boolean;
  /** Why a saved profile can't be run; absent for a usable one. */
  invalid?: string;
}

export interface ProfileExport {
  id: string;
  name: string;
  steps: StepConfig[];
  merge: MergeConfig;
  outputs?: OutputConfig;
  context_cache?: ContextCacheConfig;
  use_orientation?: boolean;
  orientation_prompt?: string;
  orientation_schema?: Record<string, unknown> | null;
  extraction?: ExtractionConfig;
  parallel_context_template?: string;
  variables?: VarSpec[];
}

export type ExportEnvelope =
  | { type: "step"; data: StepConfig }
  | {
      type: "profile";
      schema_version?: number;
      name: string;
      steps: StepConfig[];
      merge: MergeConfig;
      outputs?: OutputConfig;
      context_cache?: ContextCacheConfig;
      use_orientation?: boolean;
      orientation_prompt?: string;
      orientation_schema?: Record<string, unknown> | null;
      extraction?: ExtractionConfig;
      parallel_context_template?: string;
      variables?: VarSpec[];
    }
  | {
      type: "bundle";
      settings: Settings;
      profiles: ProfileExport[];
      active_profile: string;
    };
