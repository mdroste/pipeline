import { describe, expect, it } from "vitest";
import type { PipelineReport } from "./types";
import {
  aggregateProviderModelUsage,
  buildRunProvenance,
  provenanceMarkdown,
} from "./runProvenance";

function report(): PipelineReport {
  return {
    orientation: {
      metadata: {
        title: "Networks and Policy",
        authors: ["Ada Economist"],
        date: "",
        paper_type: "theory",
      },
      sections: [],
      formal_results: [],
      tables_figures: [],
      notation: [],
      stated_contribution: "",
      key_references: [],
      extraction_quality_notes: [],
    },
    step_outputs: [
      {
        step_id: "technical",
        step_label: "Technical",
        phase: "parallel",
        agent: "codex+claude",
        provider: "codex",
        model: "gpt-5.6-sol",
        raw_text: "",
        input_tokens: 999,
        output_tokens: 999,
        calls: [
          {
            provider: "codex",
            model: "gpt-5.6-sol",
            model_transport: "cli",
            input_tokens: 1_000,
            output_tokens: 100,
            cached_input_tokens: 700,
          },
          {
            provider: "claude",
            model: "claude-opus-4-8",
            model_transport: "api",
            input_tokens: 2_000,
            output_tokens: 200,
            cached_input_tokens: 1_500,
          },
        ],
      },
    ],
    failed_steps: [],
    report_date: "2026-08-09T12:00:00Z",
    paper_hash: "hash",
  };
}

describe("run provenance", () => {
  it("uses per-call records to split multi-provider and multi-model usage", () => {
    expect(aggregateProviderModelUsage(report())).toEqual([
      expect.objectContaining({
        provider: "Codex",
        model: "gpt-5.6-sol",
        transports: ["CLI"],
        input_tokens: 1_000,
        output_tokens: 100,
        cached_input_tokens: 700,
      }),
      expect.objectContaining({
        provider: "Claude",
        model: "claude-opus-4-8",
        transports: ["API"],
        input_tokens: 2_000,
        output_tokens: 200,
        cached_input_tokens: 1_500,
      }),
    ]);
  });

  it("keeps every provider and model visible and uses run-wide totals", () => {
    const provenance = buildRunProvenance({
      report: report(),
      runId: "run-123",
      manifest: {
        run_id: "run-123",
        profile_name: "Paper Review (Full)",
        provider: "codex",
        usage: {
          input_tokens: 3_500,
          output_tokens: 350,
          cached_input_tokens: 2_300,
        },
      },
    });

    expect(provenance.provider_summary).toBe("Codex, Claude");
    expect(provenance.model_summary).toBe("gpt-5.6-sol, claude-opus-4-8");
    expect(provenance.totals.input_tokens).toBe(3_500);
    expect(provenance.usage_matches_total).toBe(false);

    const markdown = provenanceMarkdown(provenance);
    expect(markdown).toContain("# Referee report");
    expect(markdown).toContain("Review of **Networks and Policy**");
    expect(markdown).toContain("Codex (CLI) | gpt-5.6-sol | 1,000 | 100 | 700");
    expect(markdown).toContain("Claude (API) | claude-opus-4-8 | 2,000 | 200 | 1,500");
    expect(markdown).toContain("**Run total**");
  });
});
