import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PipelinePage from "./PipelinePage";
import type {
  AutoReviewCatalog,
  PipelineConfig,
  ProfileSummary,
  StepConfig,
} from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const dialogMocks = vi.hoisted(() => ({
  save: vi.fn(),
  open: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: dialogMocks.save,
  open: dialogMocks.open,
}));

function makeConfig(): PipelineConfig {
  return {
    steps: [
      {
        id: "technical",
        label: "Technical",
        prompt: "Check the proofs.",
        enabled: true,
        phase: "parallel",
        tools: [],
        agents: ["claude"],
        context: {
          include: [
            { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
            { kind: "survey" },
          ],
        },
      },
      {
        id: "consolidate",
        label: "Consolidate Issues",
        prompt: "Merge {prior_outputs}.",
        enabled: true,
        phase: "sequential",
        tools: [],
        agents: ["claude"],
        context: {
          include: [{ kind: "step", step: "technical", parts: ["report"] }],
        },
      },
    ],
    merge: { enabled: true, prompt: "", agents: [] },
    use_orientation: true,
    orientation_prompt: "",
    extraction: { method: "" },
    parallel_context_template: "",
  };
}

const profiles: ProfileSummary[] = [
  { id: "deep-review", name: "Paper Review (Full)", step_count: 2, builtin: true },
];

function mockLoad(config: PipelineConfig, extra: Record<string, unknown> = {}) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd in extra) return Promise.resolve(extra[cmd]);
    if (cmd === "get_pipeline_config") return Promise.resolve(config);
    if (cmd === "list_profiles") return Promise.resolve(profiles);
    if (cmd === "get_active_profile") return Promise.resolve("deep-review");
    if (cmd === "save_pipeline_config") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

const macroSpecialist: StepConfig = {
  id: "subject_economics_macro",
  label: "Economics — Macroeconomics",
  prompt: "# Economics — Macroeconomics\n\nDefault host-owned macro prompt.",
  enabled: true,
  phase: "parallel",
  tools: [],
  agents: [],
  after: [],
  context: { include: [] },
};

const adaptiveCatalog: AutoReviewCatalog = {
  contract: "auto-review-v1",
  revision: "sha256:test-catalog",
  subjectCount: 1,
  methodCount: 1,
  genreCount: 10,
  disciplines: [{
    id: "economics",
    label: "Economics",
    roles: [{
      id: "subject_economics_macro",
      label: "Economics — Macroeconomics",
      level: "subfield",
      description: "Macroeconomics, policy, growth, business cycles, and aggregate dynamics.",
      exclusions: "the contribution is primarily microeconomic.",
    }],
  }],
  methodFamilies: [{
    id: "formal_conceptual",
    label: "Formal Theory & Conceptual Analysis",
    roles: [{
    id: "formal_proofs",
    label: "Method — Formal Proofs",
    level: "method",
    description: "Central theorems and derivations require proof verification.",
    exclusions: "routine algebra.",
    }],
  }],
};

async function addBlankStep(
  user: ReturnType<typeof userEvent.setup>,
  name = "Custom Step",
  phase: "parallel" | "sequential" = "parallel",
) {
  await user.click(screen.getByRole("button", { name: "+ Add step" }));
  await user.click(screen.getByRole("tab", { name: "Blank step" }));
  await user.type(screen.getByRole("textbox", { name: "Step name" }), name);
  if (phase === "sequential") {
    await user.click(screen.getByRole("radio", { name: /After earlier steps/ }));
  }
  await user.click(screen.getByRole("button", { name: "Create step" }));
}

describe("PipelinePage", () => {
  beforeEach(() => {
    invoke.mockReset();
    dialogMocks.save.mockReset();
    dialogMocks.open.mockReset();
    dialogMocks.save.mockResolvedValue(null);
    dialogMocks.open.mockResolvedValue(null);
  });

  it("loads the active profile and renders its steps", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);

    // Step labels appear in both the wave diagram and the step list.
    expect((await screen.findAllByText("Technical")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Consolidate Issues").length).toBeGreaterThan(0);
    expect(invoke).toHaveBeenCalledWith("get_pipeline_config");
    expect(invoke).toHaveBeenCalledWith("list_profiles");
  });

  it("opens the workflow gallery from the former URL-import position", async () => {
    const user = userEvent.setup();
    const onOpenGallery = vi.fn();
    mockLoad(makeConfig());
    render(
      <PipelinePage onClose={() => {}} onOpenGallery={onOpenGallery} />,
    );

    await user.click(await screen.findByRole("button", { name: "Gallery" }));

    expect(onOpenGallery).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "From URL…" })).not.toBeInTheDocument();
  });

  it("surfaces import and export dialog plugin failures", async () => {
    const user = userEvent.setup();
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findByRole("combobox", { name: "Active workflow profile" });

    dialogMocks.open.mockRejectedValueOnce(new Error("picker unavailable"));
    await user.click(screen.getByRole("button", { name: "Import" }));
    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith("Import failed: picker unavailable");
    });

    // The backend now owns the save dialog, so an export failure surfaces from
    // the export command itself rather than a JS dialog call.
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "save_pipeline_config") return Promise.resolve();
      if (cmd === "export_profile") {
        return Promise.reject(new Error("save picker unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    await user.click(screen.getByRole("button", { name: /^Export/ }));
    await user.click(screen.getByRole("button", { name: /Export profile/ }));
    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith(
        "Export failed: save picker unavailable",
      );
    });

    alertSpy.mockRestore();
  });

  it("serializes profile mutations while a switch is pending", async () => {
    const user = userEvent.setup();
    let finishSwitch!: (config: PipelineConfig) => void;
    const switchResult = new Promise<PipelineConfig>((resolve) => {
      finishSwitch = resolve;
    });
    const availableProfiles: ProfileSummary[] = [
      ...profiles,
      { id: "quick", name: "Quick Review", step_count: 1, builtin: true },
    ];
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(availableProfiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "switch_profile") return switchResult;
      if (cmd === "get_settings") return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<PipelinePage onClose={() => {}} />);

    const selector = await screen.findByRole("combobox", {
      name: "Active workflow profile",
    });
    await user.selectOptions(selector, "quick");

    expect(selector).toBeDisabled();
    expect(selector).toHaveAttribute("aria-busy", "true");
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
    expect(invoke.mock.calls.filter((call) => call[0] === "switch_profile")).toHaveLength(1);

    const quickConfig = makeConfig();
    quickConfig.steps[0].label = "Quick Technical";
    await act(async () => finishSwitch(quickConfig));

    await waitFor(() => {
      expect(selector).toBeEnabled();
      expect(selector).toHaveValue("quick");
      expect(screen.getAllByText("Quick Technical").length).toBeGreaterThan(0);
    });
  });

  it("invalidates a pending reset when the editor unmounts", async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    let finishReset!: (config: PipelineConfig) => void;
    const resetResult = new Promise<PipelineConfig>((resolve) => {
      finishReset = resolve;
    });
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "reset_pipeline_config") return resetResult;
      if (cmd === "get_settings") return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onProfileChange = vi.fn();
    const { unmount } = render(
      <PipelinePage onClose={() => {}} onProfileChange={onProfileChange} />,
    );
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Reset" }));
    unmount();
    await act(async () => finishReset(makeConfig()));

    expect(onProfileChange).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("keeps profile controls locked through deletion and fallback activation", async () => {
    const user = userEvent.setup();
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    let finishDelete!: () => void;
    const deleteResult = new Promise<void>((resolve) => {
      finishDelete = resolve;
    });
    const remaining: ProfileSummary[] = [
      { id: "auto-review", name: "Automatic Paper Review (Full)", step_count: 5, builtin: true },
    ];
    const customProfiles: ProfileSummary[] = [
      ...remaining,
      { id: "custom", name: "Custom Review", step_count: 2, builtin: false },
    ];
    let profileLists = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(makeConfig());
      if (cmd === "list_profiles") {
        profileLists += 1;
        return Promise.resolve(profileLists === 1 ? customProfiles : remaining);
      }
      if (cmd === "get_active_profile") return Promise.resolve("custom");
      if (cmd === "delete_profile") return deleteResult;
      if (cmd === "switch_profile") return Promise.resolve(makeConfig());
      if (cmd === "get_settings") return Promise.reject(new Error("not needed"));
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<PipelinePage onClose={() => {}} />);

    const selector = await screen.findByRole("combobox", {
      name: "Active workflow profile",
    });
    expect(selector).toHaveValue("custom");
    await user.click(screen.getByRole("button", { name: "Delete" }));

    expect(selector).toBeDisabled();
    expect(screen.getByRole("button", { name: "New profile" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Duplicate" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Reset" })).toBeDisabled();
    expect(invoke.mock.calls.some(([command]) => command === "switch_profile")).toBe(false);

    await act(async () => finishDelete());
    await waitFor(() => {
      expect(selector).toBeEnabled();
      expect(selector).toHaveValue("auto-review");
    });
    expect(invoke).toHaveBeenCalledWith("delete_profile", { id: "custom" });
    expect(invoke).toHaveBeenCalledWith("switch_profile", { id: "auto-review" });
    confirmSpy.mockRestore();
  });

  it("disables Save until the config is dirty", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("adding a step marks the editor dirty and saving persists the config", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await addBlankStep(user);
    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeEnabled();

    await user.click(save);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_pipeline_config",
        expect.objectContaining({
          config: expect.objectContaining({
            steps: expect.arrayContaining([
              expect.objectContaining({ id: "technical" }),
            ]),
          }),
        }),
      );
    });
    // The new step was included in the saved config.
    const saveCall = invoke.mock.calls.find(
      (c) => c[0] === "save_pipeline_config",
    );
    expect(saveCall?.[1].config.steps.length).toBe(3);
    expect(saveCall?.[1].config.steps.find(
      (step: { id: string }) => step.id === "custom_step",
    )).toMatchObject({
      after: [],
      context: {
        include: [
          { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
          { kind: "survey" },
        ],
      },
    });
  });

  it("guides step creation while preserving the full editable config", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "+ Add step" }));
    expect(screen.getByRole("dialog", { name: "Add workflow step" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create step" })).toBeDisabled();
    await user.type(
      screen.getByRole("textbox", { name: "Step name" }),
      "Identification audit",
    );
    await user.type(
      screen.getByRole("textbox", { name: "What should this step do?" }),
      "Check whether each empirical claim follows from the stated design.",
    );
    await user.click(screen.getByRole("radio", { name: /After earlier steps/ }));
    await user.click(screen.getByRole("radio", { name: /Structured issues/ }));
    await user.click(screen.getByRole("button", { name: "Create step" }));

    expect(screen.getByRole("textbox", { name: "Step label" })).toHaveValue("Identification audit");
    expect(screen.getByTestId("step-summary")).toHaveTextContent(
      "Runs after its selected dependencies using the profile’s default provider",
    );
    expect(screen.getByTestId("step-summary")).toHaveTextContent("Produces structured issues JSON");
    expect(screen.getByRole("tab", { name: "Prompt" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: "Inputs & dependencies" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Execution rules" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Model & agents" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const added = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "identification_audit",
      );
      expect(added).toMatchObject({
        label: "Identification audit",
        phase: "sequential",
        context: {
          include: [
            { kind: "survey" },
            { kind: "step", step: "technical", parts: ["report"] },
            { kind: "step", step: "consolidate", parts: ["report"] },
          ],
        },
        output_schema: expect.objectContaining({ required: ["issues"] }),
      });
      expect(saveCall?.[1].config.outputs).toEqual({
        primary_step: "identification_audit",
        findings_step: "identification_audit",
      });
    });
  }, 20_000);

  it("lets a sequential step publish its report and findings contract", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Consolidate Issues");

    await user.click(screen.getAllByRole("button", { name: "Consolidate Issues" })[0]);
    await user.click(screen.getByRole("tab", { name: "Execution rules" }));
    await user.click(screen.getByRole("checkbox", { name: /Use as the primary report/ }));
    await user.click(screen.getByRole("checkbox", { name: /Publish findings to Projects/ }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config.outputs).toEqual({
        primary_step: "consolidate",
        findings_step: "consolidate",
      });
      expect(saveCall?.[1].config.steps[1].output_schema).toMatchObject({
        required: ["issues"],
      });
    });
  });

  it("copies an adaptive agent into an independently editable workflow step", async () => {
    const user = userEvent.setup();
    const defaultPrompt = macroSpecialist.prompt;
    mockLoad(makeConfig(), {
      get_auto_review_catalog: adaptiveCatalog,
      get_auto_review_specialist_step: macroSpecialist,
    });
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "+ Add step" }));
    await user.click(screen.getByRole("tab", { name: "Adaptive agent" }));
    expect(await screen.findByText("Choose an adaptive agent")).toBeVisible();
    expect(screen.getByRole("tab", { name: "Guided setup" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Blank step" })).toBeInTheDocument();

    const search = screen.getByRole("searchbox", { name: "Search adaptive agents" });
    await user.type(search, "macro");
    await user.click(screen.getByRole("button", { name: "Economics — Macroeconomics" }));
    expect(screen.getByText(/adds an editable snapshot/i)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Copy step" }));

    const prompt = await screen.findByRole("textbox", {
      name: "Prompt for Economics — Macroeconomics",
    });
    expect(prompt).toHaveValue(defaultPrompt);
    await user.clear(prompt);
    await user.type(prompt, "My workflow-local macro prompt.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const copied = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "manual_subject_economics_macro",
      );
      expect(copied).toMatchObject({
        label: "Economics — Macroeconomics",
        prompt: "My workflow-local macro prompt.",
        phase: "parallel",
        context: {
          include: [
            { kind: "primary", parts: ["text", "structure", "visuals", "source"] },
            { kind: "survey" },
          ],
        },
      });
    });
    expect(macroSpecialist.prompt).toBe(defaultPrompt);
    expect(invoke).toHaveBeenCalledWith("get_auto_review_specialist_step", {
      id: "subject_economics_macro",
    });
  });

  it("does not mark edits made during a save as persisted", async () => {
    const user = userEvent.setup();
    let finishSave!: () => void;
    const pendingSave = new Promise<void>((resolve) => {
      finishSave = resolve;
    });
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "save_pipeline_config") return pendingSave;
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onDirtyChange = vi.fn();
    render(
      <PipelinePage onClose={() => {}} onDirtyChange={onDirtyChange} />,
    );
    await screen.findAllByText("Technical");

    await addBlankStep(user, "First custom step");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(screen.getByRole("button", { name: "Saving..." })).toBeDisabled();

    await addBlankStep(user, "Second custom step");
    await act(async () => finishSave());

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
      expect(onDirtyChange).toHaveBeenLastCalledWith(true);
    });
    expect(screen.queryByText("Saved.")).not.toBeInTheDocument();
  });

  it("edits the exact artifact allowlist for a step", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getAllByRole("button", { name: "Technical" })[0]);
    await user.click(screen.getByRole("tab", { name: "Inputs & dependencies" }));
    expect(screen.queryByRole("button", { name: "Prior reports" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Report" })).not.toBeInTheDocument();
    const readableText = screen.getByRole("checkbox", { name: "Readable text" });
    expect(readableText).toBeChecked();
    await user.click(readableText);

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const technical = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "technical",
      );
      expect(technical.context.include).toContainEqual({
        kind: "primary",
        parts: ["structure", "visuals", "source"],
      });
    });
  });

  it("preserves step-output access when a step changes phase", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Consolidate Issues");

    await user.click(screen.getAllByRole("button", { name: "Consolidate Issues" })[0]);
    await user.click(screen.getByRole("tab", { name: "Execution rules" }));
    await user.click(screen.getByRole("button", { name: "Independently (parallel)" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      const consolidate = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "consolidate",
      );
      expect(consolidate).toMatchObject({
        phase: "parallel",
        context: {
          include: [{ kind: "step", step: "technical", parts: ["report"] }],
        },
      });
    });
  });

  it("offers keyboard-operable sequential step reordering", async () => {
    const config = makeConfig();
    config.steps.push({
      id: "validate",
      label: "Validate Feedback",
      prompt: "",
      enabled: true,
      phase: "sequential",
      tools: [],
      agents: [],
      context: { include: [] },
    });
    mockLoad(config);
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Move Consolidate Issues down" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config.steps.map((step: { id: string }) => step.id)).toEqual([
        "technical",
        "validate",
        "consolidate",
      ]);
    });
  });

  it("shows 24px reorder controls only for sequential steps and no drag handles", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    const { container } = render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(
      screen.getByRole("combobox", { name: "Active workflow profile" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Move Technical up" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Move Technical down" })).not.toBeInTheDocument();
    expect(screen.queryByTitle("Drag to reorder")).not.toBeInTheDocument();
    expect(container.querySelector('[draggable="true"]')).toBeNull();

    const moveDown = screen.getByRole("button", { name: "Move Consolidate Issues down" });
    expect(moveDown).toHaveClass("h-6", "w-6");

    await user.click(screen.getAllByRole("button", { name: "Technical" })[0]);
    expect(screen.getByRole("textbox", { name: "Step label" })).toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Prompt for Technical" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Input & extraction" }));
    expect(
      screen.getByRole("combobox", { name: "Workflow input mode" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "PDF extraction method" }),
    ).toBeInTheDocument();
  });

  it("makes dependency rewrites explicit and reversible", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    mockLoad(makeConfig());
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    const enabled = screen.getByRole("switch", { name: "Enable Technical" });
    await user.click(enabled);
    expect(confirmSpy).toHaveBeenCalledWith(
      "Disabling this step removes dependencies or artifact access from 1 downstream step. Continue?",
    );
    expect(screen.getByRole("button", { name: "Undo" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(enabled).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    confirmSpy.mockRestore();
  });

  it("does not let an async prompt reset overwrite edits made while it loads", async () => {
    let resolveTemplate!: (value: string) => void;
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "get_settings") return Promise.reject(new Error("not needed"));
      if (cmd === "get_default_prompt") {
        return new Promise<string>((resolve) => {
          resolveTemplate = resolve;
        });
      }
      if (cmd === "save_pipeline_config") return Promise.resolve();
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Pipeline Settings" }));
    await user.click(screen.getByRole("button", { name: "Reset to generic" }));
    await user.click(screen.getByRole("switch", { name: "Reuse shared input context" }));
    await act(async () => resolveTemplate("new default template"));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config).toMatchObject({
        context_cache: { enabled: true },
        parallel_context_template: "new default template",
      });
    });
  });

  it("surfaces failures while loading bundled prompt defaults", async () => {
    const config = makeConfig();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_pipeline_config") return Promise.resolve(config);
      if (cmd === "list_profiles") return Promise.resolve(profiles);
      if (cmd === "get_active_profile") return Promise.resolve("deep-review");
      if (cmd === "get_orientation_defaults") {
        return Promise.reject(new Error("orientation prompt unavailable"));
      }
      if (cmd === "get_default_prompt") {
        return Promise.reject(new Error("parallel template unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const alertSpy = vi.spyOn(window, "alert").mockImplementation(() => {});
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Pipeline Settings" }));
    await user.click(screen.getByRole("button", { name: "Reset to generic" }));
    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith(
        "Failed to reset the parallel context template: parallel template unavailable",
      );
    });

    alertSpy.mockClear();
    await user.click(screen.getByRole("button", { name: "Orientation map" }));
    await user.click(screen.getByRole("button", { name: "Insert generic survey" }));
    await waitFor(() => {
      expect(alertSpy).toHaveBeenCalledWith(
        "Failed to load the default orientation prompt: orientation prompt unavailable",
      );
    });

    alertSpy.mockRestore();
  });

  it("applies stock orientation prompts and schemas as one contract", async () => {
    const config = makeConfig();
    const schema = {
      type: "object",
      required: ["overview"],
      properties: { overview: { type: "string" } },
    };
    mockLoad(config, {
      get_orientation_defaults: {
        prompt: "Build the generic survey. {input_text}",
        schema,
      },
    });
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Orientation map" }));
    await user.click(screen.getByRole("button", { name: "Insert generic survey" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config).toMatchObject({
        orientation_prompt: "Build the generic survey. {input_text}",
        orientation_schema: schema,
      });
    });
  });

  it("reports dirty state to the shell and can hide its local back control", async () => {
    const user = userEvent.setup();
    const onDirtyChange = vi.fn();
    mockLoad(makeConfig());
    const { unmount } = render(
      <PipelinePage
        onClose={() => {}}
        onDirtyChange={onDirtyChange}
        showBack={false}
      />,
    );
    await screen.findAllByText("Technical");

    expect(onDirtyChange).toHaveBeenCalledWith(false);
    expect(screen.queryByRole("button", { name: "Back" })).not.toBeInTheDocument();

    await addBlankStep(user);
    await waitFor(() => expect(onDirtyChange).toHaveBeenCalledWith(true));

    unmount();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it("shows shared context reuse as the Automatic Paper Review default", async () => {
    const user = userEvent.setup();
    mockLoad(
      {
        ...makeConfig(),
        context_cache: { enabled: true },
      },
      { get_active_profile: "auto-review" },
    );
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Pipeline Settings" }));
    const toggle = screen.getByRole("switch", { name: "Reuse shared input context" });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Automatic review default")).toBeInTheDocument();
    expect(screen.getByText(/Automatic Paper Review enables this by default/)).toBeInTheDocument();
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-checked", "false");

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_pipeline_config",
        expect.objectContaining({
          config: expect.objectContaining({
            context_cache: { enabled: false },
          }),
        }),
      );
    });
  });

  it("shows orientation as an editable workflow stage without a disable control", async () => {
    const user = userEvent.setup();
    mockLoad({ ...makeConfig(), use_orientation: false });
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    const orientation = screen.getByRole("button", { name: "Orientation map" });
    expect(orientation).not.toHaveTextContent("Required");
    await user.click(orientation);

    expect(screen.getByText("Runs before every workflow")).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Build orientation map" })).not.toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Orientation map prompt" })).toBeInTheDocument();
  });

  it("offers an overview without duplicating it in the primary step list", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    expect(screen.queryByText("Workflow overview")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Overview" }));
    expect(screen.getByText("Workflow overview")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Extract" })).toBeInTheDocument();
  });

  it("keeps effective artifact schemas visible in a dedicated low-clutter view", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("tab", { name: "Schemas" }));
    expect(screen.getByRole("tab", { name: "Schemas" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("button", { name: "Orientation schema" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Technical output schema" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Consolidate Issues output schema" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Orientation map schema" })).toBeInTheDocument();
    expect(screen.getByLabelText("Effective orientation schema")).toHaveTextContent('"type": "object"');

    await user.click(screen.getByRole("button", { name: "Technical output schema" }));
    expect(screen.getByRole("heading", { name: "Technical output schema" })).toBeInTheDocument();
    expect(screen.getByText(/stores only its content field as the step’s Markdown report/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Effective schema for Technical")).toHaveTextContent('"content"');
    expect(screen.queryByRole("textbox", { name: "Output JSON schema for Technical" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Create custom JSON schema" }));
    const editor = screen.getByRole("textbox", { name: "Output JSON schema for Technical" });
    expect((editor as HTMLTextAreaElement).value).toContain('"result"');
    fireEvent.change(editor, {
      target: {
        value: '{"type":"object","required":["score"],"properties":{"score":{"type":"number"}}}',
      },
    });
    expect(screen.getByText("Valid portable Pipeline artifact schema.")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config.steps[0].output_schema).toEqual({
        type: "object",
        required: ["score"],
        properties: { score: { type: "number" } },
      });
    });
  });

  it("keeps invalid schema drafts local and moves schema editing out of execution rules", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.steps[0].output_schema = {
      type: "object",
      required: ["result"],
      properties: { result: { type: "string" } },
    };
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("tab", { name: "Schemas" }));
    await user.click(screen.getByRole("button", { name: "Technical output schema" }));
    const editor = screen.getByRole("textbox", { name: "Output JSON schema for Technical" });
    fireEvent.change(editor, {
      target: {
        value: '{"type":"object","required":["score"],"properties":{"score":{"type":"number"}}}',
      },
    });
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
    fireEvent.change(editor, {
      target: { value: '{"type":"object","additionalProperties":false}' },
    });
    expect(screen.getByRole("alert")).toHaveTextContent("unsupported keyword");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    fireEvent.change(editor, {
      target: { value: '{"type":"array","items":{"type":"string"}}' },
    });
    expect(screen.getByRole("alert")).toHaveTextContent("root must explicitly be 'object'");
    fireEvent.change(editor, { target: { value: '{"type":' } });
    expect(screen.getByRole("alert")).toHaveTextContent("Invalid schema");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();

    await user.click(screen.getByRole("tab", { name: "Steps" }));
    await user.click(screen.getByRole("tab", { name: "Execution rules" }));
    expect(screen.queryByRole("textbox", { name: "Output JSON schema" })).not.toBeInTheDocument();
    expect(screen.getByText(/Output contracts live in the Schemas tab/)).toBeInTheDocument();
  });

  it("keeps Auto Review schemas compact while exposing the resolved provider contract", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = {
      "x-pipeline-contract": "auto-review-v1",
      "x-pipeline-catalog-policy": "live",
      type: "object",
      properties: {
        review_plan: {
          type: "object",
          properties: {
            subject_specialist_ids: {
              type: "array",
              items: { type: "string", "x-pipeline-catalog": "auto-review.subjects" },
            },
          },
        },
      },
    };
    const resolved = {
      type: "object",
      properties: {
        review_plan: {
          type: "object",
          properties: {
            subject_specialist_ids: {
              type: "array",
              items: { type: "string", enum: ["subject_economics_macro"] },
            },
          },
        },
      },
    };
    mockLoad(config, {
      get_active_profile: "auto-review",
      get_auto_review_catalog: adaptiveCatalog,
      resolve_orientation_schema_catalogs: resolved,
    });
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("tab", { name: "Schemas" }));
    expect(screen.getByText("Catalog-backed")).toBeInTheDocument();
    expect(await screen.findByText("Live specialist catalog")).toBeVisible();
    expect(screen.getByText("1 subject roles")).toBeVisible();
    expect(screen.getByText("1 method roles")).toBeVisible();
    expect(screen.getByText("10 document genres")).toBeVisible();
    expect(screen.getByRole("button", { name: "Browse catalog" })).toBeVisible();
    const editor = screen.getByRole("textbox", { name: "Orientation output JSON schema" });
    expect((editor as HTMLTextAreaElement).value).toContain('"x-pipeline-catalog": "auto-review.subjects"');
    expect((editor as HTMLTextAreaElement).value).not.toContain("subject_economics_macro");

    await user.click(screen.getByRole("button", { name: "View resolved provider schema" }));
    const resolvedPreview = await screen.findByLabelText("Resolved orientation provider schema");
    expect(resolvedPreview).toHaveTextContent("subject_economics_macro");
    expect(invoke).toHaveBeenCalledWith("resolve_orientation_schema_catalogs", {
      schema: config.orientation_schema,
    });

    const withoutContract = structuredClone(config.orientation_schema);
    delete withoutContract["x-pipeline-contract"];
    fireEvent.change(editor, { target: { value: JSON.stringify(withoutContract) } });
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Automatic Paper Review must retain x-pipeline-contract",
    );
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows compact Auto Review slots and their direct synthesis contract", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = { "x-pipeline-contract": "auto-review-v1" };
    config.orientation_prompt = [
      "SUBJECT SPECIALIST CATALOG",
      "{subject_catalog}",
      "METHOD SPECIALIST CATALOG",
      "{method_catalog}",
      "{genre_catalog}",
      "<paper>{paper_text}</paper>",
    ].join("\n\n");
    config.steps = [
      { ...config.steps[0], id: "auto_contribution", label: "Contribution & Literature" },
      { ...config.steps[0], id: "auto_consistency", label: "Claims & Consistency" },
      { ...config.steps[0], id: "auto_exposition", label: "Exposition & Architecture" },
      { ...config.steps[1], id: "auto_synthesis", label: "Consolidate" },
    ];
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);

    const orientation = await screen.findByRole("button", { name: "Orientation & classification" });
    expect(screen.getByRole("button", { name: "Adaptive agents — Automatic, 2–6 agents" })).toBeInTheDocument();
    expect(screen.getAllByText("Auto-filled from orientation").length).toBeGreaterThan(0);

    await user.click(orientation);
    expect(screen.getByRole("heading", { name: "Orientation & Classification" })).toBeInTheDocument();
    expect(screen.getByText(/One LLM call builds the paper orientation map/)).toBeInTheDocument();
    const promptEditor = screen.getByRole("textbox", { name: "Orientation map prompt" });
    expect((promptEditor as HTMLTextAreaElement).value).toContain("{subject_catalog}");
    expect((promptEditor as HTMLTextAreaElement).value).not.toContain("subject_physics_general");
    expect(screen.getByText(/Catalog placeholders are populated from the live specialist manifests/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Adaptive agents — Automatic, 2–6 agents" }));
    expect(screen.getByRole("heading", { name: "Adaptive agents" })).toBeInTheDocument();
    expect(screen.getByText(/Every adaptive-agent report feeds directly/)).toBeInTheDocument();
    expect(screen.getByText(/alongside Contribution & Literature/)).toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "Number of adaptive agents" }), "4");
    expect(screen.getByRole("button", { name: "Adaptive agents — 4 agents" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Browse method catalog" }));
    expect(screen.getByRole("dialog", { name: "Specialist catalog" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Methods" })).toHaveAttribute("aria-selected", "true");
    await user.click(screen.getByRole("button", { name: "Close specialist catalog" }));

    await user.click(screen.getByRole("tab", { name: "Overview" }));
    expect(screen.getByRole("button", { name: "Orient + classify" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Adaptive agents (4)" })).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Steps" }));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find((call) => call[0] === "save_pipeline_config");
      expect(saveCall?.[1].config.orientation_schema).toMatchObject({
        "x-pipeline-contract": "auto-review-v1",
        "x-pipeline-adaptive-agent-count": 4,
      });
    });
  });

  it("uses the Quick adaptive range and omits contribution from synthesis", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.orientation_schema = {
      "x-pipeline-contract": "auto-review-v1",
      properties: {
        review_plan: {
          properties: {
            subject_specialist_ids: { minItems: 1, maxItems: 2 },
            method_specialist_ids: { minItems: 1, maxItems: 2 },
          },
        },
      },
    };
    config.steps = [
      { ...config.steps[0], id: "auto_consistency", label: "Claims & Consistency" },
      { ...config.steps[0], id: "auto_exposition", label: "Exposition & Architecture" },
      { ...config.steps[1], id: "auto_synthesis", label: "Consolidate Feedback" },
    ];
    mockLoad(config, { get_active_profile: "auto-review-quick" });
    render(<PipelinePage onClose={() => {}} />);

    const adaptive = await screen.findByRole("button", {
      name: "Adaptive agents — Automatic, 2–4 agents",
    });
    await user.click(adaptive);

    const count = screen.getByRole("combobox", { name: "Number of adaptive agents" });
    expect(screen.getByRole("option", { name: "Automatic (2–4)" })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: "5" })).not.toBeInTheDocument();
    expect(screen.getByText(/1 to 2 method reviewers/)).toBeInTheDocument();
    expect(screen.getByText(/alongside Claims & Consistency, Exposition & Architecture/)).toBeInTheDocument();
    expect(screen.queryByText(/Contribution & Literature/)).not.toBeInTheDocument();

    await user.selectOptions(count, "4");
    expect(screen.getByRole("button", { name: "Adaptive agents — 4 agents" })).toBeInTheDocument();
  });
});
