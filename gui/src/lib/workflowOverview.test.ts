import { describe, expect, it } from "vitest";
import {
  buildOverview,
  preparationKind,
  preparationTitle,
} from "./workflowOverview";
import type { MergeConfig, StepConfig } from "./types";

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

const merge: MergeConfig = { enabled: true, prompt: "", agents: ["claude"] };

describe("buildOverview", () => {
  it("numbers preparation first and each execution wave after it", () => {
    const model = buildOverview({
      steps: [
        step({
          id: "first",
          label: "First reading",
          agents: ["claude", "codex"],
        }),
        step({ id: "methods", label: "Methods", agents: ["claude", "codex"] }),
        step({
          id: "lit",
          label: "Literature",
          phase: "sequential",
          after: ["first"],
        }),
        step({
          id: "report",
          label: "Report",
          phase: "sequential",
          after: ["lit"],
        }),
      ],
      merge,
    });

    expect(
      model.stages.map((stage) => [stage.kind, stage.number, stage.title]),
    ).toEqual([
      ["prepare", 1, "Prepare document bundle"],
      ["parallel", 2, "In parallel"],
      ["merge", null, "Merge"],
      ["single", 3, "Literature"],
      ["single", 4, "Report"],
    ]);
    expect(model.stages[0].nodes.map((node) => node.id)).toEqual([
      "extraction",
      "orientation",
    ]);
    expect(model.stages[1].nodes.map((node) => node.caption)).toEqual([
      "Claude + ChatGPT",
      "Claude + ChatGPT",
    ]);
  });

  it("counts provider calls including per-step merges and fan-out", () => {
    const model = buildOverview({
      steps: [
        step({ id: "a", agents: ["claude", "codex"] }),
        step({
          id: "b",
          agents: ["codex"],
          for_each: { glob: "*.csv", max: 4 },
        }),
        step({
          id: "c",
          phase: "sequential",
          agents: ["claude"],
          after: ["a", "b"],
        }),
      ],
      merge,
    });
    // a: 2 agents + 1 merge, b: 4 fan-out calls, c: 1.
    expect(model.providerCalls).toBe(8);
    expect(model.stepCount).toBe(3);
  });

  it("counts no model call for a Pipeline check", () => {
    const model = buildOverview({
      steps: [
        step({ id: "review", agents: ["claude"] }),
        step({
          id: "references",
          agents: ["claude", "codex"],
          kind: "analyzer",
          analyzer: { id: "reference_audit", version: 1 },
        }),
      ],
      merge,
    });
    expect(model.providerCalls).toBe(1);
    expect(model.stages[1].nodes[1]).toMatchObject({
      caption: "Pipeline check",
      agents: [],
    });
  });

  it("omits the merge stage when merge is disabled or no step has two agents", () => {
    const disabled = buildOverview({
      steps: [
        step({ id: "a", agents: ["claude", "codex"] }),
        step({ id: "b" }),
      ],
      merge: { ...merge, enabled: false },
    });
    const singleAgents = buildOverview({
      steps: [step({ id: "a", agents: ["claude"] }), step({ id: "b" })],
      merge,
    });
    expect(disabled.stages.some((stage) => stage.kind === "merge")).toBe(false);
    expect(singleAgents.stages.some((stage) => stage.kind === "merge")).toBe(
      false,
    );
  });

  it("draws a one-step parallel wave as a single numbered stage", () => {
    const model = buildOverview({
      steps: [step({ id: "solo", label: "Solo" })],
      merge,
    });
    expect(model.stages[1]).toMatchObject({
      kind: "single",
      number: 2,
      title: "Solo",
    });
  });

  it("adds the adaptive agents node to the first parallel wave only", () => {
    const model = buildOverview({
      steps: [
        step({ id: "generalist" }),
        step({ id: "validate", phase: "sequential", after: ["generalist"] }),
        step({ id: "late", after: ["validate"] }),
      ],
      merge,
      adaptiveReview: true,
      adaptiveAgentCount: 4,
    });
    const nodeIds = model.stages.flatMap((stage) =>
      stage.nodes.map((node) => node.id),
    );
    expect(nodeIds.filter((id) => id === "auto_adaptive_agents")).toHaveLength(
      1,
    );
    expect(model.stages[1]).toMatchObject({ kind: "parallel", number: 2 });
    expect(model.stages[1].nodes[1]).toMatchObject({
      label: "Adaptive agents",
      adaptive: "4",
    });
    expect(model.stages[0].nodes[1].label).toBe("Orient + classify");
  });

  it("describes run_if guards and web tools on step nodes", () => {
    const model = buildOverview({
      steps: [
        step({
          id: "report",
          label: "Report",
          phase: "sequential",
          tools: ["web"],
        }),
        step({
          id: "escalate",
          phase: "sequential",
          after: ["report"],
          run_if: { kind: "output_matches", step: "report", pattern: "reject" },
        }),
      ],
      merge,
    });
    expect(model.stages[1].nodes[0].tools).toEqual(["web"]);
    expect(model.stages[2].nodes[0].condition).toBe(
      "Runs only if Report matches “reject”",
    );
  });
});

describe("preparation naming", () => {
  it("never calls a folder input a document bundle", () => {
    expect(preparationTitle(preparationKind(""))).toBe(
      "Prepare document bundle",
    );
    expect(preparationTitle(preparationKind("document"))).toBe(
      "Prepare document bundle",
    );
    expect(preparationTitle(preparationKind("folder"))).toBe(
      "Prepare source-tree inventory",
    );
    expect(preparationTitle(preparationKind("none"))).toBe(
      "Prepare review context",
    );
  });
});
