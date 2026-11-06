import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ChainPreview } from "./Budget";
import BudgetPreview from "../self-discovery/BudgetPreview";
import { newDiscovery } from "../../lib/discoveryClient";
import type { Chain } from "../../lib/taskTypes";
import { invoke } from "@tauri-apps/api/core";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

it("explains the milestone the action limit misses before starting", async () => {
  vi.mocked(invoke).mockResolvedValue({
    batches: 15,
    teamActions: 0,
    beforeAssessment: 31,
    screenedAtLimit: 20,
    firstAssessment: 32,
    shortlistMin: 82,
    portfolioMin: 119,
    conditionalHigh: 304,
    deepLimit: 20,
    reachable: false,
  });
  render(
    <BudgetPreview
      definition={{
        ...newDiscovery(),
        maxActions: 20,
        acquireLiterature: false,
      }}
    />,
  );
  expect(
    await screen.findByText(
      "Raise the limit to at least 119 for the full run, or keep this limit for partial results.",
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/earliest project assessment is action 32/),
  ).toBeInTheDocument();
  expect(invoke).toHaveBeenCalledWith("discovery_budget_preview", {
    definition: expect.objectContaining({ candidateCount: 75, maxActions: 20 }),
  });
});

it("explains which parent or group limit will actually apply", () => {
  const limits = { maxActions: 10, actionTimeoutSecs: 60, deadlineHours: 2 };
  const group: Chain = {
    schemaVersion: 1,
    name: "group",
    description: "",
    limits: { maxActions: 1, actionTimeoutSecs: 90, deadlineHours: 1 },
    steps: [
      { id: "a", label: "A", kind: "delay", seconds: 0 },
      { id: "b", label: "B", kind: "delay", seconds: 0 },
    ],
  };
  const chain: Chain = {
    ...group,
    limits,
    steps: [{ id: "g", label: "Local group", kind: "chain", chain: group }],
  };
  render(<ChainPreview chain={chain} />);
  expect(
    screen.getByText(
      /up to 1 action · 1 h from group entry · 60 s per active step/,
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByText(/This group cannot finish within its action limit/),
  ).toBeInTheDocument();
});
