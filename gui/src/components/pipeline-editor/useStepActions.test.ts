import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import stepEditing from "../../../tests/fixtures/workflow/step-editing.json";
import stepRemoval from "../../../tests/fixtures/workflow/step-removal.json";
import { computeWaves } from "../../lib/pipelineHelpers";
import type { PipelineConfig, StepConfig } from "../../lib/types";
import {
  MAX_STEP_ID_CHARS,
  MAX_STEP_LABEL_CHARS,
  referencingSteps,
  uniqueStepId,
  withoutStepReferences,
} from "./editorState";
import { useStepActions } from "./useStepActions";
import { conditionUpstreamIds } from "./utils";
import type { WorkflowEditor } from "./useWorkflowEditor";

const dialogs = vi.hoisted(() => ({
  confirmDialog: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../DialogService", () => dialogs);

beforeEach(() => {
  vi.clearAllMocks();
  dialogs.confirmDialog.mockResolvedValue(true);
});

type Removal = (typeof stepRemoval)["grant-review"];
type Graph = Removal["source"];

function actions(config: Graph | PipelineConfig | object) {
  const setConfig = vi.fn();
  const editor = {
    config,
    editing: null,
    setConfig,
    setDirty: vi.fn(),
    setEditing: vi.fn(),
    identity: {
      activeProfileRef: { current: "" },
      promptResetRequestRef: { current: 0 },
    },
  } as unknown as WorkflowEditor;
  const { result } = renderHook(() => useStepActions(editor, vi.fn()));
  return { actions: result.current, setConfig };
}
const saved = (setConfig: ReturnType<typeof vi.fn>, previous?: object) => {
  const update = setConfig.mock.calls[0][0];
  const { steps, outputs } =
    typeof update === "function" ? update(previous) : update;
  return JSON.parse(JSON.stringify({ steps, outputs }));
};
const profiles = Object.entries(stepRemoval) as [string, Removal][];
const cases = profiles.flatMap(([name, profile]) =>
  profile.cases.map((item) => ({ name, source: profile.source, item })),
);

// The backend half of this contract is step_removal.rs, which holds each result
// below to the validators a save runs.
describe("taking a step out of a built-in workflow", () => {
  it.each(cases)(
    "$name disables $item.step into the graph the backend accepts",
    async ({ source, item }) => {
      const { actions: step, setConfig } = actions(source);
      await act(() => step.updateStepEnabled(item.step, false));
      expect(saved(setConfig)).toEqual(item.disabled);
    },
  );

  it.each(cases)(
    "$name removes $item.step into the graph the backend accepts",
    async ({ source, item }) => {
      const { actions: step, setConfig } = actions(source);
      await act(() => step.removeStep(item.step));
      expect(saved(setConfig)).toEqual(item.removed);
    },
  );
});

// The backend half of this contract is step_editing.rs, which holds each result
// below to the validators a save runs.
describe("edits the designer offers save", () => {
  it("shares the backend's id and label limits", () => {
    expect(stepEditing.limits).toEqual({
      step_id_chars: MAX_STEP_ID_CHARS,
      step_label_chars: MAX_STEP_LABEL_CHARS,
    });
  });

  it("lets a check publish findings without giving it a schema", async () => {
    const { step, source, ticked, unticked } = stepEditing.findings_role;
    const ticking = actions(source);
    act(() => ticking.actions.updateOutputRole(step, "findings_step", true));
    expect(saved(ticking.setConfig, source)).toEqual(ticked);

    const unticking = actions(ticked);
    act(() => unticking.actions.updateOutputRole(step, "findings_step", false));
    expect(saved(unticking.setConfig, ticked)).toEqual(unticked);
  });

  it("clears a schema left on a check by an older draft", () => {
    const { step, ticked } = stepEditing.findings_role;
    const stale = {
      ...ticked,
      steps: ticked.steps.map((candidate) =>
        candidate.id === step
          ? { ...candidate, output_schema: { type: "object" } }
          : candidate,
      ),
    };
    const { actions: edit, setConfig } = actions(stale);
    act(() => edit.updateOutputRole(step, "findings_step", false));
    const [check] = saved(setConfig, stale).steps;
    expect(check.output_schema).toBeNull();
  });

  it("still adds the issues schema to a model step", () => {
    const { source } = stepEditing.findings_role;
    const { actions: edit, setConfig } = actions(source);
    act(() => edit.updateOutputRole("report", "findings_step", true));
    const { steps } = saved(setConfig, source);
    expect(steps[1].output_schema).toMatchObject({ type: "object" });
  });

  it.each(stepEditing.sequential)(
    "switches a step with $name to sequential",
    async ({ step, source, result, confirms }) => {
      const { actions: edit, setConfig } = actions(source);
      await act(() => edit.updateStepPhase(step, "sequential"));
      expect(dialogs.confirmDialog).toHaveBeenCalledTimes(confirms ? 1 : 0);
      expect(saved(setConfig, source)).toEqual(result);
    },
  );

  it("names what switching to sequential gives up", async () => {
    const [{ step, source }] = stepEditing.sequential;
    const { actions: edit } = actions(source);
    await act(() => edit.updateStepPhase(step, "sequential"));
    expect(dialogs.confirmDialog.mock.calls[0][0]).toBe(
      "A sequential step runs once with one agent, so switching keeps only the first agent and turns off “Run once for each item”. Continue?",
    );
  });

  it("keeps the step unchanged if the switch is declined", async () => {
    dialogs.confirmDialog.mockResolvedValue(false);
    const [{ step, source }] = stepEditing.sequential;
    const { actions: edit, setConfig } = actions(source);
    await act(() => edit.updateStepPhase(step, "sequential"));
    expect(setConfig).not.toHaveBeenCalled();
  });

  it("cuts a long step name to an id that fits", async () => {
    const { label, source, result } = stepEditing.long_label;
    const { actions: edit, setConfig } = actions(source);
    await act(() =>
      edit.addStep({
        mode: "blank",
        label,
        instructions: "",
        phase: "parallel",
        contextPreset: "standard",
        output: "report",
      }),
    );
    expect(saved(setConfig)).toEqual(result);
  });

  it.each(stepEditing.step_ids)(
    "keeps a unique id for $base within the limit",
    ({ base, taken, id }) => {
      const steps = taken.map((other) => ({ id: other })) as StepConfig[];
      expect(uniqueStepId(base, steps)).toBe(id);
    },
  );
});

describe("what the designer tells the researcher", () => {
  const grant = stepRemoval["grant-review"].source;
  const graph = (steps: Partial<StepConfig>[], outputs = {}) =>
    ({
      outputs,
      steps: steps.map((step) => ({
        label: step.id,
        enabled: true,
        phase: "parallel",
        context: { include: [] },
        ...step,
      })),
    }) as unknown as PipelineConfig;

  it("counts required, quorum, fan-out and preserved-findings references", () => {
    expect(
      referencingSteps(grant as unknown as PipelineConfig, "grant_aims").map(
        (step) => step.id,
      ),
    ).toEqual(["grant_synthesis"]);
    const config = graph([
      { id: "source" },
      { id: "after", after: ["source"] },
      { id: "required", dependency_policy: { required: ["source"] } },
      {
        id: "quorum",
        dependency_policy: { quorum: ["source"], minimum_successes: 1 },
      },
      {
        id: "repeat",
        for_each: { glob: "", max: 5, artifact: { step: "source" } },
      },
      {
        id: "keeps",
        output_schema: { "x-pipeline-preserve-findings-from": "source" },
      },
      { id: "unrelated", dependency_policy: { required: ["after"] } },
    ]);
    expect(referencingSteps(config, "source").map((step) => step.id)).toEqual([
      "after",
      "required",
      "quorum",
      "repeat",
      "keeps",
    ]);
  });

  it("names downstream steps and named outputs in both dialogs", async () => {
    const config = graph(
      [
        { id: "source" },
        { id: "next", dependency_policy: { required: ["source"] } },
      ],
      { named: [{ key: "notes", step: "source" }] },
    );
    const disabling = actions(config).actions;
    await act(() => disabling.updateStepEnabled("source", false));
    expect(dialogs.confirmDialog.mock.calls[0][0]).toBe(
      "Disabling this step removes dependencies or artifact access from 1 downstream step and 1 named output. Continue?",
    );
    const removing = actions(config).actions;
    await act(() => removing.removeStep("source"));
    expect(dialogs.confirmDialog.mock.calls[1][0]).toBe(
      "Remove “source”? It will also remove references from 1 downstream step and 1 named output.",
    );
  });

  it("asks before dropping a named output even with no downstream step", async () => {
    const config = graph([{ id: "source" }], {
      named: [{ key: "notes", step: "source" }],
    });
    dialogs.confirmDialog.mockResolvedValue(false);
    const { actions: step, setConfig } = actions(config);
    await act(() => step.updateStepEnabled("source", false));
    expect(dialogs.confirmDialog.mock.calls[0][0]).toBe(
      "Disabling this step removes 1 named output. Continue?",
    );
    expect(setConfig).not.toHaveBeenCalled();
  });

  it.each(["updateStepEnabled", "removeStep"] as const)(
    "%s refuses to leave a fan-out without its source, and says why",
    async (action) => {
      const config = graph([
        { id: "source" },
        {
          id: "repeat",
          label: "Check each finding",
          for_each: { glob: "", max: 5, artifact: { step: "source" } },
        },
      ]);
      const { actions: step, setConfig } = actions(config);
      await act(() =>
        action === "removeStep"
          ? step.removeStep("source")
          : step.updateStepEnabled("source", false),
      );
      expect(setConfig).not.toHaveBeenCalled();
      expect(dialogs.confirmDialog).not.toHaveBeenCalled();
      expect(dialogs.notify.mock.calls[0][0]).toContain(
        "“Check each finding” runs once for each item in this step’s results.",
      );
    },
  );

  it("lets a disabled fan-out step keep its stale source", async () => {
    const config = graph([
      { id: "source" },
      {
        id: "repeat",
        enabled: false,
        for_each: { glob: "", max: 5, artifact: { step: "source" } },
      },
    ]);
    const { actions: step, setConfig } = actions(config);
    await act(() => step.updateStepEnabled("source", false));
    expect(dialogs.notify).not.toHaveBeenCalled();
    expect(setConfig).toHaveBeenCalled();
  });

  it("empties a quorum to a zero minimum", () => {
    const [step] = graph([
      {
        id: "joined",
        dependency_policy: {
          required: ["other"],
          quorum: ["source"],
          minimum_successes: 1,
        },
      },
    ]).steps;
    expect(withoutStepReferences(step, "source").dependency_policy).toEqual({
      required: ["other"],
      quorum: [],
      minimum_successes: 0,
    });
  });

  it("leaves another step's preserved findings and other schema keys alone", () => {
    const schema = {
      type: "object",
      "x-pipeline-preserve-findings-from": "kept",
    };
    const [step] = graph([{ id: "next", output_schema: schema }]).steps;
    expect(withoutStepReferences(step, "source").output_schema).toBe(schema);
    expect(withoutStepReferences(step, "kept").output_schema).toEqual({
      type: "object",
    });
  });
});

describe("the graph the designer draws", () => {
  const step = (patch: Partial<StepConfig>) =>
    ({
      label: patch.id,
      enabled: true,
      phase: "parallel",
      context: { include: [] },
      ...patch,
    }) as StepConfig;
  const steps = [
    step({ id: "source" }),
    step({ id: "other" }),
    step({ id: "required", dependency_policy: { required: ["source"] } }),
    step({
      id: "quorum",
      dependency_policy: { quorum: ["other"], minimum_successes: 1 },
    }),
    step({
      id: "repeat",
      for_each: { glob: "", max: 5, artifact: { step: "required" } },
    }),
  ];

  it("runs a step after its policy dependencies", () => {
    const waves = computeWaves(steps);
    expect(
      waves.map((wave) =>
        wave.kind === "parallel" ? wave.steps.map((s) => s.id) : [wave.step.id],
      ),
    ).toEqual([["source", "other"], ["required", "quorum"], ["repeat"]]);
  });

  it("offers fan-out and policy upstreams to the condition picker", () => {
    expect(conditionUpstreamIds(steps, "required")).toEqual(["source"]);
    expect(conditionUpstreamIds(steps, "quorum")).toEqual(["other"]);
    expect(conditionUpstreamIds(steps, "repeat")).toEqual([
      "source",
      "required",
    ]);
  });
});
