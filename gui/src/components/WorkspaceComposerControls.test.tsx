import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import WorkspaceComposerControls, {
  claudeUsageLine,
  usageLine,
} from "./WorkspaceComposerControls";
import type { WorkspaceModel } from "../lib/workbenchTypes";

vi.mock("../lib/workbench/workspaceClient", () => ({ workspaceClient: {} }));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: {},
}));
vi.mock("../lib/workbench/harnessClient", () => ({ harnessClient: {} }));
vi.mock("../lib/workbench/researchClient", () => ({ researchClient: {} }));
vi.mock("../lib/workbench/executionClient", () => ({ executionClient: {} }));
vi.mock("../lib/workbench/recipeClient", () => ({ recipeClient: {} }));
vi.mock("../lib/workbench/releaseClient", () => ({ releaseClient: {} }));
vi.mock("../lib/workbench/runtimeClient", () => ({ runtimeClient: {} }));

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
    screen.getByRole("button", { name: "Model and thinking: ChatGPT · Auto" }),
  ).toBeInTheDocument();
  open();
  expect(screen.getByRole("option", { name: /Automatic/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.getByRole("combobox", { name: "Thinking" })).toBeDisabled();
  const example = screen.getByRole("option", { name: /Example/ });
  fireEvent.mouseEnter(example);
  expect(screen.getByText("Careful reasoning for research")).toBeVisible();
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
  // Thinking stays in reach after a model is chosen.
  const thinking = screen.getByRole("combobox", { name: "Thinking" });
  expect(thinking).toHaveTextContent("Default · Medium");
  fireEvent.click(thinking);
  expect(
    within(screen.getByRole("listbox", { name: "Thinking" }))
      .getAllByRole("option")
      .map((option) => option.getAttribute("data-label")),
  ).toEqual(["Default · Medium", "Medium", "High"]);
  expect(
    screen.getByRole("button", {
      name: "Model and thinking: ChatGPT · Example · Medium",
    }),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("option", { name: "High" }));
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
  expect(screen.getByText("Careful reasoning for research")).toBeVisible();
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
      name: "Model and thinking: ChatGPT · removed-model · Ultra",
    }),
  ).toHaveAttribute("data-warning", "true");
  open();
  const removed = screen.getByRole("option", { name: /removed-model/ });
  expect(removed).toHaveAttribute("aria-selected", "true");
  expect(removed).toHaveTextContent("Unavailable");
  const unavailable = screen.getByRole("combobox", { name: "Thinking" });
  expect(unavailable).toHaveTextContent("Ultra · unavailable");
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
  expect(screen.getByRole("combobox", { name: "Thinking" })).toBeDisabled();
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
  expect(
    await screen.findByText("researcher@example.edu · Plus"),
  ).not.toBeVisible();
  fireEvent.click(screen.getByText("Account & usage"));
  expect(screen.getByText("62% of your 5-hour limit left")).toBeVisible();
});

it("offers Claude subscription mode and hides ChatGPT account information", async () => {
  const change = vi.fn();
  const load = vi.fn();
  render(
    <WorkspaceComposerControls
      models={[]}
      model=""
      effort=""
      disabled={false}
      onChange={vi.fn()}
      runtime="claude"
      onRuntimeChange={change}
      loadAccount={load}
    />,
  );
  open();
  expect(
    screen.getByRole("combobox", { name: "Conversation provider" }),
  ).toHaveTextContent("Claude Code");
  fireEvent.click(
    screen.getByRole("combobox", { name: "Conversation provider" }),
  );
  expect(load).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("option", { name: "ChatGPT" }));
  expect(change).toHaveBeenCalledWith("codex");
});

it("shows Claude usage once Claude Code has reported it", async () => {
  const usage = vi.fn().mockResolvedValue({
    status: "allowed_warning",
    resetsAt: null,
    rateLimitType: "five_hour",
    usedPercent: 82.4,
    isUsingOverage: false,
    observedAt: Date.now(),
  });
  render(
    <WorkspaceComposerControls
      models={[]}
      model=""
      effort=""
      disabled={false}
      onChange={vi.fn()}
      runtime="claude"
      loadClaudeUsage={usage}
    />,
  );
  expect(usage).not.toHaveBeenCalled();
  open();
  await screen.findByText("18% of your 5-hour limit left");
  fireEvent.click(screen.getByText("Claude usage"));
  expect(screen.getByText(/Reported by Claude Code at/)).toBeVisible();
});

it("explains when Claude has not reported usage yet", async () => {
  render(
    <WorkspaceComposerControls
      models={[]}
      model=""
      effort=""
      disabled={false}
      onChange={vi.fn()}
      runtime="claude"
      loadClaudeUsage={vi.fn().mockResolvedValue(null)}
    />,
  );
  open();
  await screen.findByText(/It will appear here after Claude's next reply/);
});

it("summarizes Claude usage reports without inventing percentages", () => {
  const now = 1_000_000_000_000;
  const report = {
    status: "allowed" as const,
    resetsAt: null,
    rateLimitType: null,
    usedPercent: null,
    isUsingOverage: false,
    observedAt: now,
  };
  expect(claudeUsageLine(null, now)).toBeNull();
  expect(claudeUsageLine(report, now)).toBe("Within your usage limit");
  expect(
    claudeUsageLine(
      { ...report, status: "allowed_warning", rateLimitType: "seven_day" },
      now,
    ),
  ).toBe("Approaching your weekly usage limit");
  expect(claudeUsageLine({ ...report, status: "rejected" }, now)).toBe(
    "Usage limit reached",
  );
  expect(claudeUsageLine({ ...report, isUsingOverage: true }, now)).toBe(
    "Using extra usage",
  );
  // A window that has already reset makes the report stale.
  expect(
    claudeUsageLine({ ...report, resetsAt: now / 1000 - 60 }, now),
  ).toBeNull();
});

it("distinguishes usage windows and never treats an expired report as a fresh balance", () => {
  const now = 2_000_000_000_000;
  const summary = usageLine(
    {
      source: "perBucket",
      observedAt: now - 60_000,
      cached: true,
      buckets: [
        {
          limitId: "codex",
          limitName: "Codex",
          planType: null,
          primary: {
            usedPercent: 30,
            remainingPercent: 70,
            windowDurationMins: 300,
            resetsAt: now / 1000 - 1,
          },
          secondary: {
            usedPercent: 20,
            remainingPercent: 80,
            windowDurationMins: 10080,
            resetsAt: null,
          },
        },
      ],
    },
    now,
  );
  expect(summary).toContain("5-hour usage needs a fresh report");
  expect(summary).toContain("80% of your weekly limit left");
  expect(summary).not.toContain("70%");
});

it("shows each Claude window with its own observation time", async () => {
  const report = {
    status: "allowed",
    resetsAt: null,
    rateLimitType: "five_hour",
    usedPercent: 40,
    isUsingOverage: false,
    observedAt: Date.now(),
  };
  const weekly = {
    ...report,
    rateLimitType: "seven_day",
    usedPercent: 10,
    observedAt: Date.now() - 3600_000,
  };
  render(
    <WorkspaceComposerControls
      models={[]}
      model=""
      effort=""
      disabled={false}
      onChange={vi.fn()}
      runtime="claude"
      loadClaudeUsage={vi
        .fn()
        .mockResolvedValue({ ...report, windows: [report, weekly] })}
    />,
  );
  open();
  await screen.findByText("60% of your 5-hour limit left");
  fireEvent.click(screen.getByText("Claude usage"));
  expect(screen.getByText("90% of your weekly limit left")).toBeVisible();
  expect(screen.getAllByText(/Reported by Claude Code at/)).toHaveLength(2);
});

it("offers API and custom connections with an editable model ID", () => {
  const change = vi.fn();
  const runtimeChange = vi.fn();
  render(
    <WorkspaceComposerControls
      models={[]}
      model="custom-model"
      effort=""
      disabled={false}
      runtime="local"
      onChange={change}
      onRuntimeChange={runtimeChange}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: /Model and thinking/ }));
  fireEvent.click(
    screen.getByRole("combobox", { name: "Conversation provider" }),
  );
  for (const name of [
    "OpenAI API",
    "Anthropic API",
    "Gemini API",
    "Local / custom",
  ]) {
    expect(screen.getByRole("option", { name })).toBeInTheDocument();
  }
  fireEvent.click(screen.getByRole("option", { name: "OpenAI API" }));
  expect(runtimeChange).toHaveBeenCalledWith("openai-api");
  fireEvent.change(screen.getByRole("textbox", { name: "Model ID" }), {
    target: { value: "another-model" },
  });
  expect(change).toHaveBeenCalledWith("another-model", "");
  expect(screen.queryByText(/No longer offered/)).not.toBeInTheDocument();
});

it("filters long catalogs and keeps keyboard selection and dismissal usable", () => {
  const change = vi.fn();
  const catalog = Array.from({ length: 20 }, (_, i) => ({
    ...models[0],
    id: `model-${i}`,
    model: `model-${i}`,
    displayName: `Model ${i}`,
  }));
  render(
    <WorkspaceComposerControls
      models={catalog}
      model="model-0"
      effort="high"
      disabled={false}
      onChange={change}
    />,
  );
  open();
  const search = screen.getByRole("textbox", { name: "Search models" });
  fireEvent.change(search, { target: { value: "model-19" } });
  expect(
    within(screen.getByRole("listbox", { name: "Model" })).getAllByRole(
      "option",
    ),
  ).toHaveLength(1);
  fireEvent.keyDown(search, { key: "ArrowDown" });
  const result = screen.getByRole("option", { name: /Model 19/ });
  expect(result).toHaveFocus();
  fireEvent.keyDown(result, { key: "Enter" });
  expect(change).toHaveBeenLastCalledWith("model-19", "");
  fireEvent.change(search, { target: { value: "no-match" } });
  expect(screen.getByRole("status")).toHaveTextContent("No matching models.");
  fireEvent.keyDown(search, { key: "Escape" });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: /^Model and thinking/ }),
  ).toHaveFocus();
  open();
  expect(screen.getByRole("textbox", { name: "Search models" })).toHaveValue(
    "",
  );
});

it("closes only the thinking menu on Escape and restores its focus", () => {
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
  fireEvent.click(screen.getByRole("combobox", { name: "Thinking" }));
  expect(screen.getByText("Balanced speed and depth")).toBeVisible();
  fireEvent.keyDown(screen.getByRole("listbox", { name: "Thinking" }), {
    key: "Escape",
  });
  expect(
    screen.getByRole("dialog", { name: "Model and thinking" }),
  ).toBeVisible();
  expect(screen.getByRole("combobox", { name: "Thinking" })).toHaveFocus();
});
