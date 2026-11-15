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

it("moves navigator focus without switching panels, then activates with Enter", async () => {
  const user = userEvent.setup();
  render(<PipelinePage onClose={() => {}} />);
  const list = await screen.findByRole("tablist", {
    name: "Review navigator view",
  });
  const steps = within(list).getByRole("tab", { name: "Steps" });
  const overview = within(list).getByRole("tab", { name: "Overview" });
  steps.focus();
  await user.keyboard("{End}");
  expect(overview).toHaveFocus();
  expect(steps).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tabpanel", { name: "Steps" })).toBeVisible();
  expect(
    screen.queryByRole("tabpanel", { name: "Overview" }),
  ).not.toBeInTheDocument();
  await user.keyboard("{Enter}");
  expect(screen.getByRole("tabpanel", { name: "Overview" })).toHaveAttribute(
    "id",
    overview.getAttribute("aria-controls"),
  );
  await user.keyboard("{Home}");
  expect(steps).toHaveFocus();
  expect(overview).toHaveAttribute("aria-selected", "true");
  await user.keyboard(" ");
  expect(steps).toHaveAttribute("aria-selected", "true");
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
