import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import ReviewRunView from "./ReviewRunView";
import { queuedRun } from "../test/queuedRunFixture";

function renderView(overrides: Partial<ComponentProps<typeof ReviewRunView>>) {
  const props: ComponentProps<typeof ReviewRunView> = {
    run: null,
    runs: [],
    limit: 2,
    state: { kind: "idle" },
    plan: null,
    stageHistory: [],
    runStartedAt: null,
    passTimes: {},
    reviewRouting: null,
    onCancel: vi.fn(),
    onNewReview: vi.fn(),
    onOpenHistory: vi.fn(),
    ...overrides,
  };
  render(<ReviewRunView {...props} />);
  return props;
}

describe("ReviewRunView", () => {
  it("explains a queued review and lets it leave the queue", async () => {
    const user = userEvent.setup();
    const runs = [
      queuedRun("a", "running"),
      queuedRun("b", "running"),
      queuedRun("draft", "queued"),
    ];
    const props = renderView({ run: runs[2], runs });

    expect(
      screen.getByRole("heading", { name: "draft.pdf is queued" }),
    ).toBeVisible();
    expect(
      screen.getByText(/one of the 2 running reviews finishes/),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Remove from queue" }));
    expect(props.onCancel).toHaveBeenCalledOnce();
    await user.click(
      screen.getByRole("button", { name: "Set up another review" }),
    );
    expect(props.onNewReview).toHaveBeenCalledOnce();
  });

  it("shows a running review's progress with its name and cancel action", async () => {
    const user = userEvent.setup();
    const props = renderView({
      run: queuedRun("draft", "running"),
      state: { kind: "extracting" },
    });

    expect(screen.getByText("draft.pdf")).toBeVisible();
    expect(
      screen.getByText(/keeps running if you leave this page/),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Cancel review" }));
    expect(props.onCancel).toHaveBeenCalledOnce();
  });

  it("centers progress with auto margins so a tall view stays scrollable from its top", () => {
    renderView({
      run: queuedRun("draft", "running"),
      state: { kind: "extracting" },
    });

    // Cross-axis centering (items-center) overflows tall content above the
    // scroll origin; auto margins collapse to zero instead.
    const content = screen.getByTestId("review-run-content");
    expect(content).toHaveClass("my-auto");
    expect(content.parentElement).toHaveClass("min-h-full");
    expect(content.parentElement).not.toHaveClass("items-center");
    expect(content.parentElement).not.toHaveClass("h-full");
  });

  it("shows a failure known only from the queue without inventing stages", () => {
    renderView({
      run: queuedRun("draft", "failed", { error: "usage limit reached" }),
      state: { kind: "error", message: "usage limit reached" },
    });

    expect(screen.getByRole("alert")).toHaveTextContent("usage limit reached");
    expect(screen.queryByText("Progress")).not.toBeInTheDocument();
  });

  it("sends finished reviews whose result left memory to history", async () => {
    const user = userEvent.setup();
    const props = renderView({
      run: queuedRun("draft", "done", {
        resultAvailable: false,
        runId: "20261026-1",
      }),
    });

    await user.click(screen.getByRole("button", { name: "Open report" }));
    expect(props.onOpenHistory).toHaveBeenCalledWith("20261026-1");
  });

  it("says when a review is no longer listed", async () => {
    const user = userEvent.setup();
    const props = renderView({ run: null });

    expect(screen.getByText("This review is no longer listed.")).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Open review history" }),
    );
    expect(props.onOpenHistory).toHaveBeenCalledWith();
  });
});
