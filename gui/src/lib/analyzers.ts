import { invoke } from "@tauri-apps/api/core";
import type { StepConfig } from "./types";

/** A host check a workflow step can run (mirrors the Rust registry). */
export interface AnalyzerInfo {
  id: string;
  version: number;
  label: string;
  description: string;
  reads: string[];
  networkServices: string[];
  networkPayload: string;
}

/** What one analyzer step reads and sends, from the run preview. */
export interface HostCheck {
  stepId: string;
  stepLabel: string;
  analyzer: string;
  reads: string[];
  networkServices: string[];
  networkPayload: string;
}

let cached: Promise<AnalyzerInfo[]> | null = null;

export function listAnalyzers(): Promise<AnalyzerInfo[]> {
  cached ??= invoke<AnalyzerInfo[]>("list_analyzers").catch((error) => {
    cached = null;
    throw error;
  });
  return cached;
}

export function isAnalyzerStep(step: StepConfig): boolean {
  return step.kind === "analyzer";
}

export function analyzerNetworkEnabled(step: StepConfig): boolean {
  return step.analyzer?.options?.network === true;
}

/** A new enabled analyzer step; it has no prompt, providers, or model. */
export function analyzerStep(
  info: AnalyzerInfo,
  id: string,
  network: boolean,
): StepConfig {
  return {
    id,
    label: info.label,
    kind: "analyzer",
    analyzer: {
      id: info.id,
      version: info.version,
      ...(info.networkServices.length > 0 ? { options: { network } } : {}),
    },
    prompt: "",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    after: [],
    context: { include: [] },
  };
}
