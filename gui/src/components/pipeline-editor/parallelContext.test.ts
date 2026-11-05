import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { PipelineConfig, StepConfig } from "../../lib/types";
import { contextForParallel } from "./parallelContext";
import { useStepActions } from "./useStepActions";
import { useWorkflowEditor } from "./useWorkflowEditor";

const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("../DialogService", () => ({ confirmDialog, notify: vi.fn() }));

function step(id: string, patch: Partial<StepConfig> = {}): StepConfig {
  return {
    id,
    label: id,
    enabled: true,
    phase: "sequential",
    tools: [],
    agents: [],
    prompt: "",
    after: [],
    context: { include: [] },
    output_schema: null,
    ...patch,
  };
}

function workflow(steps: StepConfig[]): PipelineConfig {
  return {
    steps,
    merge: { enabled: false, prompt: "", agents: [] },
    extraction: { method: "", input_mode: "document" },
  } as unknown as PipelineConfig;
}

function renderActions(config: PipelineConfig) {
  localStorage.clear();
  const hook = renderHook(() => {
    const editor = useWorkflowEditor();
    return { editor, actions: useStepActions(editor, () => {}) };
  });
  act(() => hook.result.current.editor.setConfig(config));
  return hook;
}

const readsEarlierReport = step("synthesis", {
  context: {
    include: [
      { kind: "survey" },
      { kind: "step", step: "first", parts: ["report"] },
    ],
  },
});

describe("contextForParallel", () => {
  it("removes step selections and adds the primary input", () => {
    const result = contextForParallel(readsEarlierReport.context, "document");
    expect(result.removed).toBe(1);
    expect(result.addsPrimary).toBe(true);
    expect(result.context.include).toEqual([
      { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
      { kind: "survey" },
    ]);
  });

  it("leaves a context without step selections unchanged", () => {
    const isolated = { include: [] };
    expect(contextForParallel(isolated, "document")).toEqual({
      context: isolated,
      removed: 0,
      addsPrimary: false,
    });
  });

  it("adds no primary input when the workflow has none", () => {
    const result = contextForParallel(readsEarlierReport.context, "none");
    expect(result.addsPrimary).toBe(false);
    expect(result.context.include).toEqual([{ kind: "survey" }]);
  });
});

describe("updateStepPhase", () => {
  beforeEach(() => confirmDialog.mockReset());

  it("drops hidden step selections after confirmation", async () => {
    confirmDialog.mockResolvedValue(true);
    const { result } = renderActions(
      workflow([step("first"), readsEarlierReport]),
    );
    await act(() =>
      result.current.actions.updateStepPhase("synthesis", "parallel"),
    );
    expect(confirmDialog).toHaveBeenCalledWith(
      expect.stringContaining("reads results from 1 other step"),
      { destructive: true },
    );
    const updated = result.current.editor.config?.steps[1];
    expect(updated?.phase).toBe("parallel");
    expect(
      updated?.context.include.some((selector) => selector.kind === "step"),
    ).toBe(false);
    expect(result.current.editor.dirty).toBe(true);
  });

  it("keeps the step sequential when the change is cancelled", async () => {
    confirmDialog.mockResolvedValue(false);
    const { result } = renderActions(
      workflow([step("first"), readsEarlierReport]),
    );
    await act(() =>
      result.current.actions.updateStepPhase("synthesis", "parallel"),
    );
    expect(result.current.editor.config?.steps[1]).toEqual(readsEarlierReport);
  });

  it("switches without asking when nothing is removed", async () => {
    const { result } = renderActions(workflow([step("first")]));
    await act(() =>
      result.current.actions.updateStepPhase("first", "parallel"),
    );
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(result.current.editor.config?.steps[0]).toEqual(
      step("first", { phase: "parallel" }),
    );
  });
});
