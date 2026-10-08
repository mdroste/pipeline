import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceComposerControls from "./WorkspaceComposerControls";
import type { WorkspaceModel } from "../lib/workbenchTypes";

vi.mock("../lib/workbenchClient", () => ({ workbenchClient: {} }));

const models: WorkspaceModel[] = [
  {
    id: "example",
    model: "example",
    displayName: "Example",
    description: "Careful reasoning for research",
    isDefault: true,
    defaultReasoningEffort: "medium",
    supportedReasoningEfforts: [
      { reasoningEffort: "medium", description: "Balanced speed and depth" },
      { reasoningEffort: "high", description: "High" },
    ],
  },
];
const open = () =>
  fireEvent.click(screen.getByRole("button", { name: /^Model and thinking/ }));

it("offers only supported efforts and resets effort when changing model", () => {
  const change = vi.fn();
  const view = render(
    <WorkspaceComposerControls
      models={models}
      model=""
      effort=""
      disabled={false}
      onChange={change}
    />,
  );
  expect(
    screen.getByRole("button", { name: "Model and thinking: Auto" }),
  ).toBeInTheDocument();
  open();
  expect(screen.getByRole("option", { name: /Automatic/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByRole("radio", { name: "Default" })).toBeDisabled();
  const example = screen.getByRole("option", { name: /Example/ });
  expect(example).toHaveTextContent("Careful reasoning for research");
  expect(example).toHaveTextContent("Default");
  fireEvent.click(example);
  expect(change).toHaveBeenLastCalledWith("example", "");
  view.rerender(
    <WorkspaceComposerControls
      models={models}
      model="example"
      effort=""
      disabled={false}
      onChange={change}
    />,
  );
  // The popover stays open so thinking can be chosen next.
  expect(
    within(screen.getByRole("radiogroup", { name: "Thinking" }))
      .getAllByRole("radio")
      .map((radio) => radio.textContent),
  ).toEqual(["Default", "Medium", "High"]);
  expect(screen.getByText("Uses the model's default: Medium.")).toBeVisible();
  expect(
    screen.getByRole("button", {
      name: "Model and thinking: Example · Medium",
    }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("radio", { name: "High" }));
  expect(change).toHaveBeenLastCalledWith("example", "high");
  fireEvent.click(screen.getByRole("option", { name: /Example/ }));
  expect(change).toHaveBeenCalledTimes(2);
});

it("describes the chosen level and moves between models with the keyboard", () => {
  render(
    <WorkspaceComposerControls
      models={models}
      model="example"
      effort="medium"
      disabled={false}
      onChange={vi.fn()}
    />,
  );
  open();
  expect(screen.getByText("Balanced speed and depth")).toBeVisible();
  expect(screen.getByRole("option", { name: /Example/ })).toHaveFocus();
  fireEvent.keyDown(screen.getByRole("listbox", { name: "Model" }), {
    key: "ArrowUp",
  });
  expect(screen.getByRole("option", { name: /Automatic/ })).toHaveFocus();
});

it("shows saved unavailable settings without silently selecting replacements", () => {
  const change = vi.fn();
  render(
    <WorkspaceComposerControls
      models={models}
      model="removed-model"
      effort="ultra"
      disabled={false}
      onChange={change}
    />,
  );
  expect(
    screen.getByRole("button", {
      name: "Model and thinking: removed-model · Ultra",
    }),
  ).toHaveAttribute("data-warning", "true");
  open();
  const removed = screen.getByRole("option", { name: /removed-model/ });
  expect(removed).toHaveAttribute("aria-selected", "true");
  expect(removed).toHaveTextContent("No longer offered");
  const unavailable = screen.getByRole("radio", {
    name: "Ultra · unavailable",
  });
  expect(unavailable).toBeChecked();
  expect(unavailable).toBeDisabled();
  expect(change).not.toHaveBeenCalled();
});

it("explains why choices are locked while the assistant works", () => {
  const change = vi.fn();
  render(
    <WorkspaceComposerControls
      models={models}
      model="example"
      effort=""
      disabled
      onChange={change}
    />,
  );
  open();
  expect(screen.getByRole("note")).toHaveTextContent(
    "You can change them when the conversation is idle.",
  );
  fireEvent.click(screen.getByRole("option", { name: /Automatic/ }));
  expect(screen.getByRole("radio", { name: "High" })).toBeDisabled();
  expect(change).not.toHaveBeenCalled();
});

it("shows the account and remaining usage once the picker opens", async () => {
  const load = vi.fn().mockResolvedValue({
    account: {
      status: "chatgpt",
      email: "researcher@example.edu",
      planType: "plus",
      unsupportedAccountType: null,
      requiresOpenaiAuth: false,
    },
    limits: {
      source: "perBucket",
      buckets: [
        {
          limitId: "a",
          limitName: null,
          planType: null,
          primary: {
            usedPercent: 38,
            remainingPercent: 62,
            windowDurationMins: 300,
            resetsAt: null,
          },
          secondary: null,
        },
      ],
    },
  });
  render(
    <WorkspaceComposerControls
      models={models}
      model=""
      effort=""
      disabled={false}
      onChange={vi.fn()}
      loadAccount={load}
    />,
  );
  expect(load).not.toHaveBeenCalled();
  open();
  await screen.findByText("researcher@example.edu · Plus");
  expect(screen.getByText("62% of your usage limit left")).toBeVisible();
});
