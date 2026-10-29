import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ExampleReview, { EXAMPLE_REVIEW_TITLE } from "./ExampleReview";
import { exampleIssues } from "./exampleReviewData";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

beforeEach(() => invoke.mockReset());

it("parses the bundled sample through the saved-run finding parser", () => {
  const issues = exampleIssues();
  expect(issues.map((issue) => issue.severity)).toEqual([
    "high",
    "high",
    "medium",
    "medium",
    "low",
    "low",
  ]);
  expect(new Set(issues.map((issue) => issue.section)).size).toBe(6);
  expect(
    issues.flatMap((issue) =>
      issue.evidence?.map((item) => item.verificationStatus),
    ),
  ).toEqual([
    "verified",
    "verified",
    "verified_with_normalization",
    "verified",
    "partially_verified",
    "not_found",
    "not_checkable",
  ]);
});

it("shows a clearly labeled read-only sample with severity, categories, and quote checks", async () => {
  const user = userEvent.setup();
  const onClose = vi.fn();
  render(<ExampleReview onClose={onClose} />);
  const dialog = screen.getByRole("dialog", { name: EXAMPLE_REVIEW_TITLE });
  expect(within(dialog).getByText("Sample data")).toBeVisible();
  expect(
    within(dialog).getByText(/nothing is added to your Review history/),
  ).toBeVisible();
  expect(within(dialog).getByText("Paper excerpt (fictional)")).toBeVisible();
  expect(
    within(dialog).getByRole("heading", { name: "Issues (6)" }),
  ).toBeVisible();
  expect(
    within(dialog).getAllByText("high", { selector: "span" }),
  ).toHaveLength(2);
  expect(within(dialog).getByText("§Identification")).toBeVisible();
  expect(within(dialog).getByText("§Inference")).toBeVisible();

  for (const title of [
    "Standard errors are clustered below the level of treatment",
    "The claim about mothers of young children needs a test",
    "Say which predictions of the commuting-cost model are tested",
  ])
    await user.click(
      within(dialog).getByRole("button", {
        name: (name) => name.startsWith(title),
        expanded: false,
      }),
    );
  expect(within(dialog).getByLabelText(/^Quote found\./)).toBeVisible();
  expect(within(dialog).getByLabelText(/^Partial match\./)).toBeVisible();
  expect(within(dialog).getByLabelText(/^Quote not found\./)).toBeVisible();

  // Decisions stay in memory: no saved run receives annotations.
  await user.click(
    within(dialog).getByRole("button", {
      name: "accept issue: Station locations may follow existing employment trends",
    }),
  );
  expect(within(dialog).getByText(/Annotations aren't saved/)).toBeVisible();
  expect(invoke).not.toHaveBeenCalled();

  await user.keyboard("{Escape}");
  expect(onClose).toHaveBeenCalledOnce();
});
