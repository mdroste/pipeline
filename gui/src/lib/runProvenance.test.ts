import { describe, expect, it } from "vitest";
import type { PipelineReport } from "./types";
import {
  aggregateProviderModelUsage,
  aggregateStepModelUsage,
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
            effort: "high",
            input_tokens: 1_000,
            output_tokens: 100,
            cached_input_tokens: 700,
          },
          {
            provider: "claude",
            model: "claude-opus-4-8",
            model_transport: "api",
            effort: "max",
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
        efforts: ["high"],
        input_tokens: 1_000,
        output_tokens: 100,
        cached_input_tokens: 700,
      }),
      expect.objectContaining({
        provider: "Claude",
        model: "claude-opus-4-8",
        transports: ["API"],
        efforts: ["max"],
        input_tokens: 2_000,
        output_tokens: 200,
        cached_input_tokens: 1_500,
      }),
    ]);
  });

  it("keeps step boundaries while splitting a multi-model step", () => {
    const usage = aggregateStepModelUsage(report());

    expect(usage).toEqual([
      expect.objectContaining({
        step_id: "technical",
        step_label: "Technical",
        provider: "Codex",
        model: "gpt-5.6-sol",
        input_tokens: 1_000,
        output_tokens: 100,
      }),
      expect.objectContaining({
        step_id: "technical",
        step_label: "Technical",
        provider: "Claude",
        model: "claude-opus-4-8",
        input_tokens: 2_000,
        output_tokens: 200,
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
    expect(markdown).toContain("# Pipeline");
    expect(markdown).toContain("**Document:** Networks and Policy");
    expect(markdown).toContain("**Workflow:** Paper Review (Full)");
    expect(markdown).toContain("**Token usage:** 3,500 input · 350 output · 2,300 cached");
    expect(markdown).toContain(
      "**Models / providers:** Codex (CLI) / gpt-5.6-sol / effort high; Claude (API) / claude-opus-4-8 / effort max",
    );
    expect(markdown).not.toContain("## Run provenance");
    expect(markdown).not.toContain("**Run total**");
  });

  it("survives a paper-shaped survey with no metadata or authors", () => {
    // Schema-less profiles validate the survey only as a JSON object, so a
    // paper-shaped survey (stated_contribution present) may omit metadata
    // entirely or return authors as null; the view must not throw.
    const partial = report();
    partial.orientation = {
      stated_contribution: "A new estimator",
    } as PipelineReport["orientation"];
    const provenance = buildRunProvenance({
      report: partial,
      summary: null,
      manifest: null,
      durationSecs: null,
    });
    expect(provenance.subject).toBe("Pipeline report");
    expect(provenance.authors).toEqual([]);

    const nullAuthors = report();
    (nullAuthors.orientation as Record<string, unknown>).metadata = {
      title: "T",
      authors: null,
    };
    const fromNull = buildRunProvenance({
      report: nullAuthors,
      summary: null,
      manifest: null,
      durationSecs: null,
    });
    expect(fromNull.subject).toBe("T");
    expect(fromNull.authors).toEqual([]);
  });
});
