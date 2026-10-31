import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import AgentChips from "./AgentChips";
import { describeStep } from "./stepSummary";
import type { PipelineConfig, StepConfig } from "../../lib/types";

it("uses provider names instead of internal ids", () => {
  render(<AgentChips agents={[]} onChange={vi.fn()} />);

  expect(screen.getByRole("button", { name: "ChatGPT" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Gemini" })).toBeVisible();
  expect(
    screen.queryByRole("button", { name: "Codex" }),
  ).not.toBeInTheDocument();
  expect(
    screen.queryByRole("button", { name: "Antigravity" }),
  ).not.toBeInTheDocument();
});

it("names providers in the step summary", () => {
  const step = {
    id: "review",
    label: "Review",
    prompt: "",
    enabled: true,
    phase: "parallel",
    tools: [],
    agents: ["codex", "antigravity"],
    context: { include: [] },
  } satisfies StepConfig;
  const summary = describeStep(step, {
    steps: [step],
  } as unknown as PipelineConfig);

  expect(JSON.stringify(summary)).toContain("ChatGPT and Gemini");
  expect(JSON.stringify(summary)).not.toMatch(/Codex|Antigravity/);
});
