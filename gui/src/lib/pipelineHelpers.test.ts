import { describe, it, expect } from "vitest";
import {
  computeWaves,
  findUnknownPlaceholders,
  lintCrossStepReferences,
  placeholdersFor,
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
    context: { include: [] },
    ...overrides,
  } as StepConfig;
}

describe("computeWaves", () => {
  it("groups adjacent parallel steps and isolates sequential steps", () => {
    const waves = computeWaves([
      step({ id: "a" }),
      step({ id: "b" }),
      step({
        id: "c",
        phase: "sequential",
        context: {
          include: [
            { kind: "step", step: "a", parts: ["report"] },
            { kind: "step", step: "b", parts: ["report"] },
          ],
        },
      }),
      step({
        id: "d",
        after: ["c"],
      }),
    ]);
    expect(waves.map((w) => w.kind)).toEqual([
      "parallel",
      "sequential",
      "parallel",
    ]);
    expect(
      waves[0].kind === "parallel" && waves[0].steps.map((s) => s.id),
    ).toEqual(["a", "b"]);
  });

  it("skips disabled steps unless includeDisabled is set", () => {
    const steps = [step({ id: "a" }), step({ id: "b", enabled: false })];
    expect(computeWaves(steps)[0]).toMatchObject({ steps: [{ id: "a" }] });
    expect(computeWaves(steps, true)[0]).toMatchObject({
      steps: [{ id: "a" }, { id: "b" }],
    });
  });

  it("flags multi-agent waves", () => {
    const waves = computeWaves([
      step({ id: "a", agents: ["claude", "antigravity"] }),
    ]);
    expect(waves[0]).toMatchObject({ kind: "parallel", hasMultiAgent: true });
  });

  it("places artifact fan-out after its upstream producer", () => {
    const waves = computeWaves([
      step({ id: "source" }),
      step({
        id: "consumer",
        for_each: {
          glob: "",
          max: 20,
          artifact: { step: "source", pointer: "/findings" },
        },
      }),
    ]);

    expect(waves).toHaveLength(2);
    expect(waves[0]).toMatchObject({
      kind: "parallel",
      steps: [{ id: "source" }],
    });
    expect(waves[1]).toMatchObject({
      kind: "parallel",
      steps: [{ id: "consumer" }],
    });
  });
});

describe("findUnknownPlaceholders", () => {
  it("accepts valid tokens for the context and flags unknown ones", () => {
    const ctx = { kind: "sequential" as const, otherStepIds: ["technical"] };
    const text =
      "Use {prior_outputs} and {step:technical} but not {bogus_token}.";
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

  it("recognizes live catalog placeholders only for Auto Review orientation prompts", () => {
    const prompt = "{subject_catalog} {method_catalog} {genre_catalog}";
    expect(
      findUnknownPlaceholders(prompt, {
        kind: "orientation",
        autoReview: true,
      }),
    ).toHaveLength(0);
    expect(
      findUnknownPlaceholders(prompt, { kind: "orientation" }),
    ).toHaveLength(3);
  });

  it("permits draft {step:...} references in sequential prompts", () => {
    const hits = findUnknownPlaceholders("{step:not-yet-created}", {
      kind: "sequential",
      otherStepIds: [],
    });
    expect(hits).toHaveLength(0);
  });
});

describe("placeholdersFor", () => {
  it("offers {input_path} with {paper_path} kept as a legacy alias", () => {
    const seq = placeholdersFor({ kind: "sequential", otherStepIds: [] }).map(
      (p) => p.token,
    );
    expect(seq).toContain("{input_path}");
    expect(seq).toContain("{paper_path}");
    const tmpl = placeholdersFor({ kind: "parallel_template" }).map(
      (p) => p.token,
    );
    expect(tmpl).toContain("{input_path}");
    expect(tmpl).toContain("{paper_path}");
  });

  it("returns no placeholders for parallel step prompts", () => {
    expect(placeholdersFor({ kind: "parallel" })).toEqual([]);
  });

  it("offers runtime catalog placeholders for Auto Review orientation prompts", () => {
    const tokens = placeholdersFor({
      kind: "orientation",
      autoReview: true,
    }).map((p) => p.token);
    expect(tokens).toContain("{subject_catalog}");
    expect(tokens).toContain("{method_catalog}");
    expect(tokens).toContain("{genre_catalog}");
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

describe("lintCrossStepReferences", () => {
  const baseConfig = (steps: Partial<StepConfig>[]) =>
    ({
      steps: steps.map((step, index) => ({
        id: `s${index}`,
        label: `s${index}`,
        prompt: "",
        enabled: true,
        phase: "parallel",
        tools: [],
        agents: [],
        after: [],
        context: { include: [] },
        ...step,
      })),
      orientation_schema: {
        type: "object",
        properties: {
          metadata: {
            type: "object",
            properties: { title: { type: "string" } },
          },
          sections: {
            type: "array",
            items: {
              type: "object",
              properties: { title: { type: "string" } },
            },
          },
        },
      },
    }) as unknown as Parameters<typeof lintCrossStepReferences>[0];

  it("flags unknown and disabled step references in prompts", () => {
    const config = baseConfig([
      { id: "gone", enabled: false },
      {
        id: "final",
        phase: "sequential",
        prompt: "Use {step:ghost} and {step:gone}",
      },
    ]);
    const messages = lintCrossStepReferences(config).map(
      (warning) => warning.message,
    );
    expect(
      messages.some((m) => m.includes("unknown step '{step:ghost}'")),
    ).toBe(true);
    expect(
      messages.some((m) => m.includes("disabled step '{step:gone}'")),
    ).toBe(true);
  });

  it("checks survey pointers against the orientation schema", () => {
    const config = baseConfig([
      {
        id: "a",
        run_if: {
          kind: "survey_path",
          pointer: "/metadata/title",
          exists: true,
        },
      },
      {
        id: "b",
        run_if: {
          kind: "survey_path",
          pointer: "/metadata/nonexistent",
          exists: true,
        },
      },
      {
        id: "c",
        run_if: {
          kind: "survey_path",
          pointer: "/sections/0/title",
          exists: true,
        },
      },
    ]);
    const warnings = lintCrossStepReferences(config);
    expect(warnings).toHaveLength(1);
    expect(warnings[0].stepId).toBe("b");
    expect(warnings[0].message).toContain("/metadata/nonexistent");
  });

  it("notes regex conditions against structured steps", () => {
    const config = baseConfig([
      { id: "structured", output_schema: { type: "object" } },
      {
        id: "gate",
        run_if: { kind: "output_matches", step: "structured", pattern: "yes" },
      },
    ]);
    const warnings = lintCrossStepReferences(config);
    expect(warnings.some((w) => w.message.includes("canonical JSON"))).toBe(
      true,
    );
  });
});
