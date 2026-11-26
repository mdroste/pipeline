import {
  fireEvent,
  render,
  screen,
  within,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { useRef, useState } from "react";
import ExampleReview from "./ExampleReview";
import NavRail from "../NavRail";
import {
  ExampleReviewSurface,
  ExampleReviewSurfaceProvider,
  useExampleReviewSurface,
} from "./ExampleReviewSurface";
import { placeTutorialCoach } from "./useTutorialSpotlight";
import { EXAMPLE_ADAPTIVE_REVIEWERS } from "./exampleReviewData";
import TutorialCoach from "./TutorialCoach";
import { REVIEW_TUTORIAL_STEPS, TUTORIAL_STEP } from "./reviewTutorialSteps";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => {
  invoke.mockReset();
  navigate.mockReset();
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
});
const coach = () =>
  within(screen.getByRole("complementary", { name: "Review tutorial" }));
const navigate = vi.fn();
function TutorialRail({ collapsed = false }: { collapsed?: boolean }) {
  const surface = useExampleReviewSurface();
  return (
    <NavRail
      activePage={surface?.page ?? "home"}
      transientNavigation={!!surface?.page}
      hasCurrentRun={false}
      dependenciesReady
      dependenciesLoading={false}
      width={216}
      collapsed={collapsed}
      onResize={() => {}}
      onNewRun={() => {}}
      onNavigate={navigate}
      onDependencies={() => {}}
    />
  );
}
function TutorialHost({ collapsed }: { collapsed: boolean }) {
  return (
    <ExampleReviewSurfaceProvider>
      <TutorialRail collapsed={collapsed} />
      <ExampleReviewSurface />
      <ExampleReview onClose={() => {}} />
    </ExampleReviewSurfaceProvider>
  );
}
async function setupToPreview(user: ReturnType<typeof userEvent.setup>) {
  await user.click(
    within(screen.getByTestId("example-review")).getByRole("button", {
      name: /New review$/,
    }),
  );
  await screen.findByDisplayValue("Paper Review");
  await user.click(
    coach().getByRole("button", { name: "Next: review design" }),
  );
  await user.click(coach().getByRole("button", { name: "Next: instructions" }));
  const note = screen.getByRole("textbox", { name: "Additional instructions" });
  await user.clear(note);
  await user.type(note, "Check the event-study assumptions.");
  await user.click(coach().getByRole("button", { name: "Next: models" }));
  await screen.findAllByText("Example model", { selector: "option" });
  await user.click(
    coach().getByRole("button", { name: "Continue to preview" }),
  );
  await user.click(screen.getByRole("button", { name: "Preview" }));
}

it.each([false, true])(
  "uses the production controls from New review to a cited page (sidebar minimized: %s)",
  async (collapsed) => {
    const user = userEvent.setup();
    const { rerender } = render(<TutorialHost collapsed={collapsed} />);
    expect(
      screen.queryByRole("navigation", { name: "Review walkthrough chapters" }),
    ).not.toBeInTheDocument();
    expect(
      coach().queryByRole("button", { name: /New review$/ }),
    ).not.toBeInTheDocument();
    await setupToPreview(user);
    const preview = screen.getByRole("region", { name: "Ready to start" });
    expect(
      within(preview).getByText("Check the event-study assumptions."),
    ).toBeVisible();
    const models = within(preview).getByRole("region", {
      name: "Models in this review",
    });
    for (const [, label] of EXAMPLE_ADAPTIVE_REVIEWERS)
      expect(models).toHaveTextContent(label);
    await user.click(
      within(preview).getByRole("button", { name: "Start review" }),
    );
    expect(screen.getByTestId("review-run-content")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /^Cancel .*\(/ }),
    ).not.toBeInTheDocument();
    const runningText = screen.getByTestId("review-run-content").textContent;
    expect(
      coach().queryByRole("button", { name: "Go to Home" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Home" }).closest("[inert]"),
    ).toBeNull();
    expect(
      screen.getByRole("button", { name: "Settings" }).closest("[inert]"),
    ).not.toBeNull();
    // Replacing the row with its icon (or vice versa) keeps the new target live.
    rerender(<TutorialHost collapsed={!collapsed} />);
    const home = screen.getByRole("button", { name: "Home" });
    await waitFor(() => expect(home.closest("[inert]")).toBeNull());
    // Tab wraps from the coach to the external target; Shift+Tab wraps back.
    const last = coach().getByRole("button", { name: "Skip to results" });
    last.focus();
    await user.tab();
    expect(home).toHaveFocus();
    await user.tab({ shift: true });
    expect(last).toHaveFocus();
    if (collapsed) {
      home.focus();
      await user.keyboard("{Enter}");
    } else {
      await user.click(home);
    }
    expect(navigate).not.toHaveBeenCalled();
    expect(screen.queryByTestId("review-run-content")).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "What would you like to do?" }),
    ).toBeVisible();
    const resume = screen.getByRole("button", { name: /A review is running/ });
    expect(resume.closest("[inert]")).toBeNull();
    expect(
      within(screen.getByTestId("example-review"))
        .getByRole("button", { name: /New review$/ })
        .closest("[inert]"),
    ).not.toBeNull();
    await user.click(resume);
    expect(
      screen.getByRole("button", { name: "Home" }).closest("[inert]"),
    ).not.toBeNull();
    expect(screen.getByTestId("review-run-content").textContent).toBe(
      runningText,
    );
    // Back can revisit the detour and resume the same sample again.
    await user.click(coach().getByRole("button", { name: "Back" }));
    await user.click(
      screen.getByRole("button", { name: /A review is running/ }),
    );
    await user.click(
      coach().getByRole("button", { name: "Fast-forward example" }),
    );
    expect(screen.getByText("Validate Feedback")).toBeVisible();
    await user.click(
      coach().getByRole("button", { name: "Finish example review" }),
    );
    await user.click(screen.getByRole("button", { name: "Open report" }));
    await screen.findByRole("heading", {
      name: "Correctness and Internal Consistency",
    });
    const report = screen.getByRole("tabpanel");
    for (const [, label] of EXAMPLE_ADAPTIVE_REVIEWERS)
      expect(report).toHaveTextContent(label);
    await user.click(screen.getByRole("tab", { name: /Findings/ }));
    const issue = await screen.findByRole("button", {
      name: (name) => name.startsWith("The $2.30 return does not follow"),
      expanded: false,
    });
    await user.click(issue);
    // Returning to an already expanded finding must keep its evidence reachable.
    await user.click(coach().getByRole("button", { name: "Back" }));
    await user.click(screen.getByRole("tab", { name: /Findings/ }));
    await user.click(
      screen.getByRole("button", { name: "View Page 6 evidence" }),
    );
    expect(screen.getByRole("tab", { name: "Sources" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await screen.findByRole("heading", { name: "Page 6" }, { timeout: 5000 });
    await user.click(coach().getByRole("button", { name: "Explore results" }));
    expect(screen.getByText("Tutorial complete")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: /Findings/ }));
    expect(
      screen.getByRole("button", { name: "Export all decisions" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Export (example only)" }),
    ).toBeDisabled();
    expect(invoke).not.toHaveBeenCalled();
  },
);

it("keeps off-target actions inert, supports Back, and resets the sample on replay", async () => {
  const user = userEvent.setup();
  const bounds = vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: HTMLElement) {
      if (this.dataset.reviewTour === "run-preview")
        return new DOMRect(120, 60, 780, 650);
      if (this.dataset.reviewTour === "start-review")
        return new DOMRect(740, 645, 140, 40);
      if (this.classList.contains("review-tutorial-coach"))
        return new DOMRect(0, 0, 360, 300);
      return new DOMRect();
    });
  render(<ExampleReview onClose={() => {}} />);
  const other = screen.getByRole("button", { name: /New conversation$/ });
  expect(other.closest("[inert]")).not.toBeNull();
  await setupToPreview(user);
  await waitFor(() =>
    expect(
      screen.getByRole("complementary", { name: "Review tutorial" }),
    ).toHaveStyle({ left: "16px" }),
  );
  bounds.mockRestore();
  await user.click(coach().getByRole("button", { name: "Back" }));
  expect(
    screen.queryByRole("region", { name: "Ready to start" }),
  ).not.toBeInTheDocument();
  await user.click(coach().getByRole("button", { name: "Restart" }));
  expect(screen.getByRole("button", { name: /New review$/ })).toBeVisible();
  await user.click(screen.getByRole("button", { name: /New review$/ }));
  expect(
    screen.getByRole("textbox", { name: "Additional instructions" }),
  ).not.toHaveValue("Check the event-study assumptions.");
  expect(invoke).not.toHaveBeenCalled();
});

it("restores the user's mounted page, draft, and focus when the tutorial exits", async () => {
  const user = userEvent.setup();
  function Host() {
    const [open, setOpen] = useState(false);
    return (
      <ExampleReviewSurfaceProvider>
        <TutorialRail />
        <ExampleReviewSurface>
          <button onClick={() => setOpen(true)}>Open walkthrough</button>
          <input
            aria-label="Existing draft"
            defaultValue="My unfinished review"
          />
        </ExampleReviewSurface>
        {open && (
          <ExampleReview
            onClose={() => setOpen(false)}
            backLabel="Back to setup"
          />
        )}
      </ExampleReviewSurfaceProvider>
    );
  }
  render(<Host />);
  const opener = screen.getByRole("button", { name: "Open walkthrough" });
  const draft = screen.getByRole("textbox", { name: "Existing draft" });
  await user.click(opener);
  expect(draft).not.toBeVisible();
  await setupToPreview(user);
  await user.click(screen.getByRole("button", { name: "Start review" }));
  await user.click(screen.getByRole("button", { name: "Home" }));
  await user.keyboard("{Escape}");
  await waitFor(() => expect(opener).toHaveFocus());
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Existing draft" })).toBe(draft);
  expect(draft).toHaveValue("My unfinished review");
  expect(draft.closest("[inert]")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Home" }));
  expect(navigate).toHaveBeenCalledWith("home");
});

it("blocks live-app shortcuts while keeping editing shortcuts available", async () => {
  const user = userEvent.setup();
  const liveShortcut = vi.fn();
  window.addEventListener("keydown", liveShortcut);
  const { unmount } = render(<ExampleReview onClose={() => {}} />);
  fireEvent.keyDown(document, { key: "n", ctrlKey: true });
  expect(liveShortcut).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: /New review$/ }));
  await user.click(
    coach().getByRole("button", { name: "Next: review design" }),
  );
  await user.click(coach().getByRole("button", { name: "Next: instructions" }));
  liveShortcut.mockClear();
  const note = screen.getByRole("textbox", { name: "Additional instructions" });
  expect(fireEvent.keyDown(note, { key: "a", ctrlKey: true })).toBe(true);
  expect(liveShortcut).toHaveBeenCalledOnce();
  unmount();
  liveShortcut.mockClear();
  fireEvent.keyDown(document, { key: "n", ctrlKey: true });
  expect(liveShortcut).toHaveBeenCalledOnce();
  window.removeEventListener("keydown", liveShortcut);
});

it("keeps the coach away from a bottom-right target and inside a narrow viewport", () => {
  const position = placeTutorialCoach(
    { left: 950, top: 650, width: 180, height: 50 },
    360,
    280,
    1200,
    800,
  );
  expect(position).toEqual({ left: 16, top: 504 });
  const narrow = placeTutorialCoach(
    { left: 16, top: 30, width: 300, height: 100 },
    328,
    250,
    360,
    700,
  );
  expect(narrow.left).toBeGreaterThanOrEqual(16);
  expect(narrow.left + 328).toBeLessThanOrEqual(344);
  expect(narrow.top).toBeGreaterThanOrEqual(130);
});

it("keeps the source explanation in the bottom right when its spotlight fills the content area", async () => {
  let panelTop = 100;
  const bounds = vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: HTMLElement) {
      if (this.getAttribute("role") === "tabpanel")
        return new DOMRect(
          240,
          panelTop,
          window.innerWidth - 240,
          window.innerHeight,
        );
      if (this.classList.contains("review-tutorial-coach"))
        return new DOMRect(0, 0, 360, 280);
      return new DOMRect();
    });
  function SourceStep() {
    const root = useRef<HTMLDivElement>(null);
    const noop = () => {};
    return (
      <div ref={root}>
        <div role="tabpanel">Source document</div>
        <TutorialCoach
          root={root}
          steps={[REVIEW_TUTORIAL_STEPS[TUTORIAL_STEP.sources]]}
          step={0}
          label="Review tutorial"
          icon="review"
          completionTitle="Complete"
          onNext={noop}
          onBack={noop}
          onRestart={noop}
          onSkip={noop}
          onExplore={noop}
          onClose={noop}
        />
      </div>
    );
  }
  try {
    const { container } = render(<SourceStep />);
    const guide = screen.getByRole("complementary", {
      name: "Review tutorial",
    });
    const bottomRight = {
      left: `${window.innerWidth - 360 - 16}px`,
      top: `${window.innerHeight - 280 - 16}px`,
    };
    expect(guide).toHaveStyle(bottomRight);
    expect(container.querySelector(".review-tutorial-spotlight")).toHaveStyle({
      top: "94px",
    });
    panelTop = 20;
    fireEvent.scroll(screen.getByRole("tabpanel"));
    await waitFor(() =>
      expect(container.querySelector(".review-tutorial-spotlight")).toHaveStyle(
        { top: "14px" },
      ),
    );
    expect(guide).toHaveStyle(bottomRight);
  } finally {
    bounds.mockRestore();
  }
});
