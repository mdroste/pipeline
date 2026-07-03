import { describe, it, expect } from "vitest";
import {
  computeWaves,
  findUnknownPlaceholders,
  placeholdersFor,
  reorderSteps,
} from "./pipelineHelpers";
import type { StepConfig } from "./types";

function step(overrides: Partial<StepConfig> & { id: string }): StepConfig {
  return {
    label: overrides.id,
    prompt: "",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: [],
    ...overrides,
  } as StepConfig;
}

describe("computeWaves", () => {
  it("groups adjacent parallel steps and isolates sequential steps", () => {
    const waves = computeWaves([
      step({ id: "a" }),
      step({ id: "b" }),
      step({ id: "c", phase: "sequential" }),
      step({ id: "d" }),
    ]);
    expect(waves.map((w) => w.kind)).toEqual([
      "parallel",
      "sequential",
      "parallel",
    ]);
    expect(waves[0].kind === "parallel" && waves[0].steps.map((s) => s.id)).toEqual(
      ["a", "b"],
    );
  });

  it("skips disabled steps unless includeDisabled is set", () => {
    const steps = [step({ id: "a" }), step({ id: "b", enabled: false })];
    expect(computeWaves(steps)[0]).toMatchObject({ steps: [{ id: "a" }] });
    expect(computeWaves(steps, true)[0]).toMatchObject({
      steps: [{ id: "a" }, { id: "b" }],
    });
  });

  it("flags multi-agent waves", () => {
    const waves = computeWaves([step({ id: "a", agents: ["claude", "gemini"] })]);
    expect(waves[0]).toMatchObject({ kind: "parallel", hasMultiAgent: true });
  });
});

describe("findUnknownPlaceholders", () => {
  it("accepts valid tokens for the context and flags unknown ones", () => {
    const ctx = { kind: "sequential" as const, otherStepIds: ["technical"] };
    const text = "Use {prior_outputs} and {step:technical} but not {bogus_token}.";
    const hits = findUnknownPlaceholders(text, ctx);
    expect(hits).toHaveLength(1);
    expect(hits[0]).toMatchObject({ match: "{bogus_token}", line: 1 });
  });

  it("ignores LaTeX-style braces that don't look like placeholders", () => {
    const hits = findUnknownPlaceholders(
      "\\frac{1}{2} and {  spaced  } are fine",
      { kind: "merge" },
    );
    expect(hits).toHaveLength(0);
  });

  it("reports 1-based line numbers", () => {
    const hits = findUnknownPlaceholders("line one\nline {nope} two", {
      kind: "orientation",
    });
    expect(hits[0].line).toBe(2);
  });

  it("permits draft {step:...} references in sequential prompts", () => {
    const hits = findUnknownPlaceholders(
      "{step:not-yet-created}",
      { kind: "sequential", otherStepIds: [] },
    );
    expect(hits).toHaveLength(0);
  });
});

describe("placeholdersFor", () => {
  it("offers {input_path} with {paper_path} kept as a legacy alias", () => {
    const seq = placeholdersFor({ kind: "sequential", otherStepIds: [] }).map((p) => p.token);
    expect(seq).toContain("{input_path}");
    expect(seq).toContain("{paper_path}");
    const tmpl = placeholdersFor({ kind: "parallel_template" }).map((p) => p.token);
    expect(tmpl).toContain("{input_path}");
    expect(tmpl).toContain("{paper_path}");
  });


  it("returns no placeholders for parallel step prompts", () => {
    expect(placeholdersFor({ kind: "parallel" })).toEqual([]);
  });

  it("includes per-step references for sequential prompts", () => {
    const tokens = placeholdersFor({
      kind: "sequential",
      otherStepIds: ["empirical"],
    }).map((p) => p.token);
    expect(tokens).toContain("{prior_outputs}");
    expect(tokens).toContain("{step:empirical}");
  });
});

describe("reorderSteps", () => {
  const steps = [step({ id: "a" }), step({ id: "b" }), step({ id: "c" })];

  it("moves a step and applies the target phase", () => {
    const next = reorderSteps(steps, 0, 2, "sequential");
    expect(next.map((s) => s.id)).toEqual(["b", "c", "a"]);
    expect(next[2].phase).toBe("sequential");
  });

  it("clamps out-of-range destinations and rejects invalid sources", () => {
    expect(reorderSteps(steps, 1, 99, "parallel").map((s) => s.id)).toEqual([
      "a",
      "c",
      "b",
    ]);
    expect(reorderSteps(steps, -1, 0, "parallel")).toBe(steps);
  });
});
