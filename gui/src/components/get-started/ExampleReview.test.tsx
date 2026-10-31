import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { detectReportIssues, renderSpecialistMarkdown } from "../../lib/issues";
import ExampleReview, { EXAMPLE_REVIEW_TITLE } from "./ExampleReview";
import { EXAMPLE_REPORT } from "./exampleReviewData";
import { EXAMPLE_FINDINGS } from "./exampleReviewFindings";
import { EXAMPLE_PAPER_PAGES } from "./exampleReviewPaper";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

const lazyPanelWait = { timeout: 5_000 };
const scrollIntoView = vi.fn();

beforeEach(() => {
  invoke.mockReset();
  scrollIntoView.mockReset();
  Element.prototype.scrollIntoView = scrollIntoView;
});

const pageText = (page: number) =>
  EXAMPLE_PAPER_PAGES[page - 1].replace(/\\([$*])/g, "$1");

it("marks each sample quotation with what a real check of the paper would find", () => {
  const pages = EXAMPLE_PAPER_PAGES.map((_, index) => pageText(index + 1));
  const statuses = new Set<string>();
  for (const item of EXAMPLE_FINDINGS.flatMap((finding) => finding.evidence)) {
    statuses.add(item.verification_status);
    const cited = pageText(item.page);
    const quote = item.quote ?? "";
    const foundIgnoringCase = (text: string) =>
      text.toLowerCase().includes(quote.toLowerCase());
    switch (item.verification_status) {
      case "verified":
        expect(cited).toContain(quote);
        break;
      case "verified_with_normalization":
        expect(cited).not.toContain(quote);
        expect(foundIgnoringCase(cited)).toBe(true);
        break;
      case "partially_verified":
        expect(foundIgnoringCase(cited)).toBe(false);
        expect(pages.some(foundIgnoringCase)).toBe(true);
        break;
      case "not_found":
        expect(pages.some(foundIgnoringCase)).toBe(false);
        break;
      case "not_checkable":
        expect(quote).toBe("");
        break;
    }
  }
  expect(statuses).toEqual(
    new Set([
      "verified",
      "verified_with_normalization",
      "partially_verified",
      "not_found",
      "not_checkable",
    ]),
  );
});

it("builds the sample in the same shape as a saved run", () => {
  const issues = detectReportIssues(EXAMPLE_REPORT) ?? [];
  expect(issues.map((issue) => issue.severity)).toEqual(
    EXAMPLE_FINDINGS.map((finding) => finding.priority),
  );
  expect(issues[2].sources).toEqual([
    "Method — Causal Identification",
    "Economics — Labor Economics",
  ]);

  const quality = EXAMPLE_REPORT.quality;
  const evidence = EXAMPLE_FINDINGS.flatMap((finding) => finding.evidence);
  expect(
    quality!.verified_evidence +
      quality!.partially_verified_evidence +
      quality!.unverified_evidence +
      quality!.not_checkable_evidence!,
  ).toBe(evidence.length);

  // Every finding reaches a reviewer's report, and each report renders.
  const reviewers = EXAMPLE_REPORT.step_outputs.slice(0, -2);
  const reviewerIds = new Set(reviewers.map((step) => step.step_id));
  for (const finding of EXAMPLE_FINDINGS)
    for (const id of finding.reviewers) expect(reviewerIds).toContain(id);
  for (const step of reviewers)
    expect(renderSpecialistMarkdown(step.raw_text)).toContain("**#1.");
});

it("opens the real report screen on sample data and saves nothing", async () => {
  const user = userEvent.setup();
  const onClose = vi.fn();
  render(<ExampleReview onClose={onClose} />);
  const dialog = screen.getByRole("dialog", { name: EXAMPLE_REVIEW_TITLE });

  expect(
    within(dialog).getByText(/No model was called, and nothing is saved/),
  ).toBeVisible();
  expect(
    within(dialog).getByRole("heading", {
      name: "Bus Rapid Transit and Women's Employment: Evidence from Port Harlan",
    }),
  ).toBeVisible();
  expect(within(dialog).getByText("Completed report")).toBeVisible();
  expect(within(dialog).getByText("2 quotes need checking")).toBeVisible();
  expect(
    await within(dialog).findByRole(
      "heading",
      { name: "Correctness and Internal Consistency" },
      lazyPanelWait,
    ),
  ).toBeVisible();

  await user.click(within(dialog).getByRole("tab", { name: "Report details" }));
  expect(
    await within(dialog).findByText("Recorded limitations", { selector: "p" }),
  ).toBeVisible();

  await user.click(within(dialog).getByRole("tab", { name: /Issues/ }));
  await within(dialog).findByRole(
    "heading",
    { name: "Issues (10)" },
    lazyPanelWait,
  );
  expect(within(dialog).getByText(/quote check catches it/)).toBeVisible();

  for (const title of [
    "Standard errors are clustered below the level of treatment",
    "The paper's own references contradict the “first paper” claim",
    "Say which prediction of Lee and Ortiz (2017) the paper tests",
  ])
    await user.click(
      within(dialog).getByRole("button", {
        name: (name) => name.startsWith(title),
        expanded: false,
      }),
    );
  expect(within(dialog).getAllByLabelText(/^Quote found\./)).toHaveLength(2);
  expect(within(dialog).getByLabelText(/^Partial match\./)).toBeVisible();
  expect(within(dialog).getByLabelText(/^Quote not found\./)).toBeVisible();

  // Decisions stay in memory: no saved run receives annotations.
  await user.click(
    within(dialog).getByRole("button", {
      name: "accept issue: The $2.30 return does not follow from the paper's own numbers",
    }),
  );
  expect(within(dialog).getByText(/Annotations aren't saved/)).toBeVisible();

  // A cited page opens the extracted paper at that page.
  await user.click(
    within(dialog).getByRole("button", { name: "View Page 6 evidence" }),
  );
  expect(within(dialog).getByRole("tab", { name: "Sources" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  const target = await within(dialog).findByRole(
    "heading",
    { name: "Page 6" },
    lazyPanelWait,
  );
  expect(scrollIntoView.mock.contexts).toContain(target);

  expect(invoke).not.toHaveBeenCalled();

  await user.keyboard("{Escape}");
  expect(onClose).toHaveBeenCalledOnce();
});
