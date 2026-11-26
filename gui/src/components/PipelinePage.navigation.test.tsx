import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PipelinePage from "./PipelinePage";
import type { PipelineConfig } from "../lib/workflowTypes";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./DialogService", () => ({ confirmDialog: vi.fn(), notify: vi.fn() }));
const config: PipelineConfig = {
  steps: [
    {
      id: "read",
      label: "Read",
      prompt: "Read carefully.",
      enabled: true,
      phase: "parallel",
      tools: [],
      agents: ["claude"],
      context: { include: [{ kind: "primary", parts: ["text"] }] },
    },
  ],
  merge: { enabled: true, prompt: "", agents: [] },
  use_orientation: true,
  orientation_prompt: "",
  extraction: { method: "" },
  parallel_context_template: "",
};
beforeEach(() => {
  invoke.mockReset();
  invoke.mockImplementation((command: string) => {
    if (command === "get_pipeline_config") return Promise.resolve(config);
    if (command === "list_profiles")
      return Promise.resolve([
        { id: "review", name: "Review", step_count: 1, builtin: false },
      ]);
    if (command === "get_active_profile") return Promise.resolve("review");
    return Promise.resolve(null);
  });
});

it("keeps all export actions, skips disabled steps, and restores trigger focus", async () => {
  const user = userEvent.setup();
  render(<PipelinePage onClose={() => {}} />);
  const trigger = await screen.findByRole("button", { name: "Export" });
  await user.click(trigger);
  const menu = screen.getByRole("menu", { name: "Export review design" });
  const items = within(menu).getAllByRole("menuitem");
  expect(items).toHaveLength(3);
  expect(items[0]).toBeDisabled();
  expect(items[1]).toHaveFocus();
  await user.keyboard("{End}");
  expect(items[2]).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
  expect(invoke).not.toHaveBeenCalledWith("export_profile", expect.anything());
});

it("opens a stage from the overview in the step editor and preserves its draft", async () => {
  const user = userEvent.setup();
  render(<PipelinePage onClose={() => {}} initialStageId="read" />);
  const prompt = await screen.findByRole("textbox", {
    name: "Prompt for Read",
  });
  await user.type(prompt, " Cite the evidence.");
  await user.click(screen.getByRole("button", { name: "Back to overview" }));
  const diagram = screen.getByRole("region", { name: "Review overview" });
  const returningStage = within(diagram).getByRole("button", { name: "Read" });
  expect(returningStage).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(screen.getByRole("textbox", { name: "Prompt for Read" })).toHaveValue(
    "Read carefully. Cite the evidence.",
  );
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

it("opens a disabled reviewer from the picker even when the graph has no enabled steps", async () => {
  const off = {
    ...config,
    steps: config.steps.map((step) => ({ ...step, enabled: false })),
  };
  invoke.mockImplementation((command) =>
    Promise.resolve(
      command === "get_pipeline_config"
        ? off
        : command === "list_profiles"
          ? [{ id: "review", name: "Declaude" }]
          : command === "get_active_profile"
            ? "review"
            : null,
    ),
  );
  const user = userEvent.setup();
  render(<PipelinePage onClose={() => {}} />);
  const picker = await screen.findByRole("combobox", {
    name: "Choose a stage",
  });
  expect(
    screen.getByRole("combobox", { name: "Active review design" }),
  ).toHaveTextContent("Declaude");
  expect(screen.queryByRole("button", { name: "Read" })).toBeNull();
  await user.click(picker);
  await user.click(screen.getByRole("option", { name: /Read.*Disabled/ }));
  await user.click(screen.getByRole("switch", { name: "Enable Read" }));
  await user.click(screen.getByRole("button", { name: "Back to overview" }));
  expect(screen.getByRole("button", { name: "Read" })).toHaveFocus();
});
it("opens model settings and the selected step's schema from its prompt", async () => {
  const user = userEvent.setup();
  render(<PipelinePage onClose={() => {}} initialStageId="read" />);
  await user.click(await screen.findByText("Advanced settings"));
  await user.click(screen.getByRole("button", { name: "Model & agents" }));
  expect(screen.getByRole("tab", { name: "Model & agents" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await user.click(screen.getByRole("tab", { name: "Prompt" }));
  await user.click(screen.getByRole("button", { name: "Output format" }));
  expect(
    screen.getByRole("button", { name: "Read output schema" }),
  ).toHaveAttribute("aria-pressed", "true");
});
