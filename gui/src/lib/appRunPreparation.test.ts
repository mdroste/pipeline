import { describe, expect, it } from "vitest";
import { configForRunPreview } from "./appRunPreparation";
import type { PipelineConfig, StepConfig } from "./types";

const step = (id: string, patch: Partial<StepConfig>): StepConfig => ({
  id,
  label: id,
  prompt: "",
  enabled: true,
  phase: "sequential",
  tools: [],
  agents: [],
  context: { include: [] },
  ...patch,
});

describe("configForRunPreview", () => {
  it("keeps workflow model and thinking pins when only PDF reading changes", () => {
    const config = {
      merge: { agents: [] },
      steps: [
        step("review", {
          phase: "parallel",
          model: "pinned-model",
          effort: "high",
        }),
      ],
    } as unknown as PipelineConfig;
    const preview = configForRunPreview(config, ["codex"], {
      agents: [],
      model_overrides: {},
      effort_overrides: {},
      extraction_method: "pdftotext",
    });
    expect(preview.steps[0]).toMatchObject({
      agents: ["codex"],
      model: "pinned-model",
      effort: "high",
    });
    expect(config.steps[0].agents).toEqual([]);
  });
  it("fills inherited steps with the providers they will run on", () => {
    const config = {
      merge: { enabled: true, prompt: "", agents: [] },
      steps: [
        step("review", { phase: "parallel" }),
        step("write", {}),
        step("pinned", { agents: ["local"] }),
      ],
    } as unknown as PipelineConfig;

    const preview = configForRunPreview(
      config,
      ["claude", "codex"],
      null,
      "claude",
      "antigravity",
    );

    expect(preview.steps.map((item) => item.agents)).toEqual([
      ["claude", "codex"],
      ["antigravity"],
      ["local"],
    ]);
    expect(preview.merge.agents).toEqual(["claude"]);
  });
});
