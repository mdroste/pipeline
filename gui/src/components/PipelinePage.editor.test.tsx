import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  invoke,
  confirmDialog,
  makeConfig,
  mockLoad,
  macroSpecialist,
  adaptiveCatalog,
  addBlankStep,
  useAdvancedEditor,
  resetPipelinePageMocks,
} from "./PipelinePage.test-support";
import PipelinePage from "./PipelinePage";

describe("PipelinePage editor", () => {
  beforeEach(resetPipelinePageMocks);

  it("loads the active profile and renders its steps", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);

    expect((await screen.findAllByText("Technical")).length).toBeGreaterThan(0);
    expect(screen.getAllByText("Consolidate Issues").length).toBeGreaterThan(0);
    expect(invoke).toHaveBeenCalledWith("get_pipeline_config");
    expect(invoke).toHaveBeenCalledWith("list_profiles");
  });

  it("opens the requested step after the workflow loads", async () => {
    mockLoad(makeConfig());
    render(<PipelinePage onClose={vi.fn()} initialStageId="consolidate" />);
    expect(
      await screen.findByDisplayValue("Merge {prior_outputs}."),
    ).toBeVisible();
    expect(
      screen.queryByDisplayValue("Check the proofs."),
    ).not.toBeInTheDocument();
  });

  it("opens on the overview with review actions in the page header", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findByRole("combobox", { name: "Active review design" });

    expect(
      screen.getByRole("region", { name: "Review overview" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("navigation", { name: "Review outline" }),
    ).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Export/ }));
    expect(
      screen.getByRole("menu", { name: "Export review design" }),
    ).toBeVisible();
    expect(
      screen.getByRole("menuitem", { name: "Export selected step" }),
    ).toBeDisabled();
  });

  it("uses the app's word for a review in designer dialogs", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await user.click(
      await screen.findByRole("button", { name: "Design actions" }),
    );
    await user.click(
      screen.getByRole("menuitem", { name: "New review design" }),
    );
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "New review design name",
    );
    expect(screen.getByRole("dialog")).not.toHaveTextContent(/profile/i);
    await user.keyboard("{Escape}");
    await user.click(
      await screen.findByRole("button", { name: "Design actions" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "Rename review design",
    );
    expect(screen.getByRole("dialog")).not.toHaveTextContent(/profile/i);
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
    expect(
      saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "custom_step",
      ),
    ).toMatchObject({
      after: [],
      context: {
        include: [
          {
            kind: "primary",
            parts: ["text", "structure", "visuals", "source"],
          },
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
    expect(
      screen.getByRole("dialog", { name: "Add review step" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create step" })).toBeDisabled();
    await user.type(
      screen.getByRole("textbox", { name: "Step name" }),
      "Identification audit",
    );
    await user.type(
      screen.getByRole("textbox", { name: "What should this step do?" }),
      "Check whether each empirical claim follows from the stated design.",
    );
    await user.click(
      screen.getByRole("radio", { name: /After earlier steps/ }),
    );
    await user.click(screen.getByRole("radio", { name: /Structured issues/ }));
    await user.click(screen.getByRole("button", { name: "Create step" }));
    await useAdvancedEditor(user);

    expect(screen.getByRole("textbox", { name: "Step label" })).toHaveValue(
      "Identification audit",
    );
    expect(screen.getByTestId("step-summary")).toHaveTextContent(
      "Runs after its selected dependencies using the review’s default provider",
    );
    expect(screen.getByTestId("step-summary")).toHaveTextContent(
      "Produces structured issues JSON",
    );
    expect(screen.getByRole("tab", { name: "Prompt" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole("tab", { name: "Inputs & dependencies" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "When it runs" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("tab", { name: "Model & agents" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
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
    await useAdvancedEditor(user);

    await user.click(
      screen.getAllByRole("button", { name: "Consolidate Issues" })[0],
    );
    await user.click(screen.getByRole("tab", { name: "When it runs" }));
    await user.click(
      screen.getByRole("checkbox", { name: /Use as the primary report/ }),
    );
    await user.click(
      screen.getByRole("checkbox", {
        name: /Track findings in the project ledger/,
      }),
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
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
    expect(
      screen.getByRole("tab", { name: "Guided setup" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Blank step" })).toBeInTheDocument();

    const search = screen.getByRole("searchbox", {
      name: "Search adaptive agents",
    });
    await user.type(search, "macro");
    await user.click(
      screen.getByRole("button", { name: "Economics — Macroeconomics" }),
    );
    expect(
      screen.getByText(/Add a copy you can edit for this review design/i),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Copy step" }));

    const prompt = await screen.findByRole("textbox", {
      name: "Prompt for Economics — Macroeconomics",
    });
    expect(prompt).toHaveValue(defaultPrompt);
    await user.clear(prompt);
    await user.type(prompt, "My workflow-local macro prompt.");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      const copied = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "manual_subject_economics_macro",
      );
      expect(copied).toMatchObject({
        label: "Economics — Macroeconomics",
        prompt: "My workflow-local macro prompt.",
        phase: "parallel",
        context: {
          include: [
            {
              kind: "primary",
              parts: ["text", "structure", "visuals", "source"],
            },
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

  it("edits the exact artifact allowlist for a step", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await useAdvancedEditor(user);

    await user.click(screen.getAllByRole("button", { name: "Technical" })[0]);
    await user.click(
      screen.getByRole("tab", { name: "Inputs & dependencies" }),
    );
    expect(
      screen.queryByRole("button", { name: "Prior reports" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("checkbox", { name: "Report" }),
    ).not.toBeInTheDocument();
    const readableText = screen.getByRole("checkbox", {
      name: "Readable text",
    });
    expect(readableText).toBeChecked();
    await user.click(readableText);

    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      const technical = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "technical",
      );
      expect(technical.context.include).toContainEqual({
        kind: "primary",
        parts: ["structure", "visuals", "source"],
      });
    });
  });

  it("removes step-output access when a step becomes parallel", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Consolidate Issues");
    await useAdvancedEditor(user);

    await user.click(
      screen.getAllByRole("button", { name: "Consolidate Issues" })[0],
    );
    await user.click(screen.getByRole("tab", { name: "When it runs" }));
    // Validation rejects parallel steps that read other steps' output, so the
    // switch asks first and then gives the step the primary input instead.
    confirmDialog.mockResolvedValueOnce(true);
    await user.click(
      screen.getByRole("button", { name: "Independently (parallel)" }),
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      const consolidate = saveCall?.[1].config.steps.find(
        (step: { id: string }) => step.id === "consolidate",
      );
      expect(consolidate).toMatchObject({
        phase: "parallel",
        context: {
          include: [
            {
              kind: "primary",
              parts: ["text", "structure", "visuals", "source"],
            },
          ],
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

    await user.click(
      screen.getByRole("button", { name: "Consolidate Issues" }),
    );
    await user.click(screen.getByRole("button", { name: "Step actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Move down" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(
        saveCall?.[1].config.steps.map((step: { id: string }) => step.id),
      ).toEqual(["technical", "validate", "consolidate"]);
    });
  });

  it("opens step actions in the editor and limits reordering to sequential steps", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Technical" }));
    await user.click(screen.getByRole("button", { name: "Step actions" }));
    expect(screen.queryByRole("menuitem", { name: "Move up" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Remove step" })).toBeVisible();
    await user.keyboard("{Escape}");
    expect(
      screen.getByRole("textbox", { name: "Step label" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Prompt for Technical" }),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "Choose a stage" }));
    await user.click(
      screen.getByRole("option", { name: /Input & extraction/ }),
    );
    expect(
      screen.getByRole("combobox", { name: "Review input mode" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "PDF extraction method" }),
    ).toBeInTheDocument();
  });

  it("makes dependency rewrites explicit and reversible", async () => {
    confirmDialog.mockResolvedValueOnce(true);
    mockLoad(makeConfig());
    const user = userEvent.setup();
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Technical" }));
    const enabled = screen.getByRole("switch", { name: "Enable Technical" });
    await user.click(enabled);
    expect(confirmDialog.mock.calls[0][0]).toBe(
      "Disabling this step removes dependencies or artifact access from 1 downstream step. Continue?",
    );
    expect(screen.getByRole("button", { name: "Undo" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Undo" }));
    expect(enabled).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
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
    expect(
      screen.queryByRole("button", { name: "Back to review" }),
    ).not.toBeInTheDocument();

    await addBlankStep(user);
    await waitFor(() => expect(onDirtyChange).toHaveBeenCalledWith(true));

    unmount();
    expect(onDirtyChange).toHaveBeenLastCalledWith(false);
  });

  it("shows shared context reuse as the Paper Review default", async () => {
    const user = userEvent.setup();
    mockLoad(
      {
        ...makeConfig(),
        context_cache: { enabled: true },
      },
      { get_active_profile: "auto-paper-review" },
    );
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");

    await user.click(screen.getByRole("button", { name: "Review settings" }));
    const toggle = screen.getByRole("switch", {
      name: "Reuse shared input context",
    });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Built-in default")).toBeInTheDocument();
    expect(
      screen.getByText(/Paper Review and Slide Review enable this by default/),
    ).toBeInTheDocument();
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

    const orientation = screen.getByRole("button", {
      name: "Identify content",
    });
    expect(orientation).not.toHaveTextContent("Required");
    await user.click(orientation);

    expect(screen.getByText("Runs before every review")).toBeInTheDocument();
    expect(
      screen.queryByRole("switch", { name: "Build document overview" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Document overview prompt" }),
    ).toBeInTheDocument();
  });

  it("saves a comma-separated list that is still being typed on Cmd+S", async () => {
    const user = userEvent.setup();
    const config = makeConfig();
    config.variables = [
      { key: "tone", label: "Tone", kind: "choice", default: "" },
    ];
    mockLoad(config);
    render(<PipelinePage onClose={() => {}} />);
    await screen.findAllByText("Technical");
    await user.click(screen.getByRole("button", { name: "Review settings" }));
    const choices = screen.getByRole("textbox", {
      name: "Variable Tone choices",
    });
    await user.type(choices, "Minor revision, Major revision");
    await user.keyboard("{Meta>}s{/Meta}");

    await waitFor(() => {
      const saveCall = invoke.mock.calls.find(
        (call) => call[0] === "save_pipeline_config",
      );
      expect(saveCall?.[1].config.variables).toEqual([
        expect.objectContaining({
          key: "tone",
          choices: ["Minor revision", "Major revision"],
        }),
      ]);
    });
    expect(choices).toHaveFocus();
  });

  it("leaves Cmd+Z to text fields and open dialogs", async () => {
    const user = userEvent.setup();
    mockLoad(makeConfig());
    render(<PipelinePage onClose={() => {}} />);
    await user.click((await screen.findAllByText("Technical"))[0]);
    const label = await screen.findByLabelText("Step label");
    await user.type(label, "!");
    expect(label).toHaveValue("Technical!");

    // Inside a text field the browser's own undo runs instead.
    fireEvent.keyDown(label, { key: "z", metaKey: true });
    expect(label).toHaveValue("Technical!");

    // A dialog over the editor keeps the workflow unchanged.
    await user.click(
      await screen.findByRole("button", { name: "Design actions" }),
    );
    await user.click(screen.getByRole("menuitem", { name: "Rename" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.keyDown(dialog, { key: "z", metaKey: true });
    fireEvent.keyDown(dialog, { key: "s", metaKey: true });
    expect(label).toHaveValue("Technical!");
    expect(
      invoke.mock.calls.some((call) => call[0] === "save_pipeline_config"),
    ).toBe(false);
    await user.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // Outside fields and dialogs the shortcut undoes the workflow edit.
    fireEvent.keyDown(document.body, { key: "z", metaKey: true });
    expect(label).toHaveValue("Technical");
    fireEvent.keyDown(document.body, {
      key: "z",
      metaKey: true,
      shiftKey: true,
    });
    expect(label).toHaveValue("Technical!");
  });
});
