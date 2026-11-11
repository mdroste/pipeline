import { render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import PreparedModelTable from "./PreparedModelTable";
import type { RosterEntry } from "../lib/types";

it("shows the prepared model, thinking, source, extraction, and backup choices", () => {
  const entry = (patch: Partial<RosterEntry>): RosterEntry => ({
    stage: "parallel",
    label: "Review",
    step_ids: [],
    step_labels: [],
    provider: "codex",
    transport: "api",
    model: { mode: "automatic" },
    effort: "",
    source: "default",
    note: null,
    ...patch,
  });
  render(
    <PreparedModelTable
      entries={[
        entry({
          step_labels: ["Proofs"],
          model: { mode: "pinned", model: "saved-model-v2" },
          effort: "high",
          source: "workflow",
        }),
        entry({ stage: "extraction", label: "Read PDF" }),
        entry({
          stage: "fallback",
          label: "Backup",
          note: "Used when the first provider reaches its usage limit.",
        }),
      ]}
    />,
  );
  const proof = screen
    .getByRole("rowheader", { name: "Proofs" })
    .closest("tr")!;
  expect(
    within(proof).getByText("saved-model-v2", { exact: false }),
  ).toBeVisible();
  expect(within(proof).getByText("high")).toBeVisible();
  expect(
    screen.getAllByText("Automatic — chosen at launch", { exact: false }),
  ).toHaveLength(2);
  expect(screen.getByRole("rowheader", { name: "Read PDF" })).toBeVisible();
  expect(
    screen.getByText(/first provider reaches its usage limit/),
  ).toBeVisible();
});
