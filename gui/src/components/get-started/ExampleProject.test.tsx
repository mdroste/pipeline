import { useState } from "react";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import ExampleProject from "./ExampleProject";
import GetStartedHost from "./GetStartedHost";
import HomePage from "../HomePage";
import NavRail from "../NavRail";
import { openGetStarted, saveGetStarted } from "../../lib/getStarted";
import { router } from "../../lib/router";
import {
  ExampleReviewSurface,
  ExampleReviewSurfaceProvider,
  useExampleReviewSurface,
} from "./ExampleReviewSurface";
import { EXAMPLE_PAPER } from "./exampleReviewPaper";
import { PROJECT_QUESTION, PROJECT_TASK } from "./projectTutorialFixture";
import { placeTutorialCoach } from "./useTutorialSpotlight";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

it("keeps the coach below a compact dialog even when its submit button is partly scrolled out", () => {
  expect(
    placeTutorialCoach(
      { left: 10, top: 10, width: 370, height: 476 },
      358,
      288,
      390,
      800,
      { left: 200, top: 470, width: 160, height: 38 },
    ),
  ).toEqual({ left: 16, top: 496 });
});
beforeEach(() => {
  localStorage.clear();
  invoke
    .mockReset()
    .mockRejectedValue(new Error("Tutorial must not invoke native services"));
  Element.prototype.scrollIntoView = vi.fn();
});
const coach = () =>
  within(screen.getByRole("complementary", { name: "Projects tutorial" }));

function TutorialRail() {
  const surface = useExampleReviewSurface();
  return (
    <NavRail
      activePage={surface?.page ?? "home"}
      transientNavigation={!!surface?.page}
      transientProjectNavigation={surface?.projectNavigation}
      hasCurrentRun={false}
      dependenciesReady
      dependenciesLoading={false}
      width={216}
      onResize={() => {}}
      onNewRun={() => {}}
      onNavigate={() => {}}
      onDependencies={() => {}}
    />
  );
}
const next = async (user: ReturnType<typeof userEvent.setup>, name: string) =>
  user.click(coach().getByRole("button", { name }));

async function reachNotes(user: ReturnType<typeof userEvent.setup>) {
  await next(user, "Next: research brief");
  await user.click(screen.getByRole("button", { name: "Context" }));
  expect(
    screen.getByRole("group", { name: "Project context" }),
  ).toHaveTextContent(PROJECT_QUESTION);
  await next(user, "Next: project notes");
}

it("walks through the real project controls with local changes and the Reviews paper", async () => {
  const user = userEvent.setup();
  const navigate = vi.spyOn(router, "navigate");
  render(
    <ExampleReviewSurfaceProvider>
      <TutorialRail />
      <ExampleReviewSurface />
      <ExampleProject onClose={() => {}} />
    </ExampleReviewSurfaceProvider>,
  );
  const stored = { ...localStorage };
  await user.click(screen.getByRole("button", { name: "New project" }));
  await user.click(screen.getByRole("button", { name: "Choose file…" }));
  const name = screen.getByRole("textbox", { name: "Project name" });
  expect(name).toHaveValue("port-harlan-brt");
  await user.clear(name);
  await user.type(name, "My Port Harlan study");
  await user.selectOptions(
    screen.getByRole("combobox", { name: "Kind of paper" }),
    "empirical",
  );
  await user.click(screen.getByRole("button", { name: "Choose folder…" }));
  await user.click(screen.getByRole("button", { name: "Create project" }));
  expect(
    await screen.findByRole("heading", { name: "My Port Harlan study" }),
  ).toBeVisible();
  expect(screen.getByText(EXAMPLE_PAPER.title)).toBeVisible();
  expect(
    screen.getByRole("navigation", { name: "Project sections" }),
  ).toBeVisible();
  await next(user, "Next: attention");
  expect(
    screen.queryByRole("combobox", { name: "Stage of the paper" }),
  ).toBeNull();
  await next(user, "Next: documents");
  await reachNotes(user);
  await user.click(screen.getByRole("button", { name: "Edit note" }));
  await user.clear(screen.getByRole("textbox", { name: "Edit note" }));
  await user.type(
    screen.getByRole("textbox", { name: "Edit note" }),
    "Check whether the event study identifies an employment effect.",
  );
  await user.click(screen.getByRole("button", { name: "Save changes" }));
  await user.click(screen.getByRole("button", { name: "Suggested" }));
  await user.click(screen.getByRole("button", { name: "Accept" }));
  expect(
    screen.queryByRole("button", { name: "Accept" }),
  ).not.toBeInTheDocument();
  await next(user, "Next: action items");
  expect(
    screen.queryByRole("combobox", { name: "Stage of the paper" }),
  ).toBeNull();
  await user.click(screen.getByRole("button", { name: /Action items/ }));
  await user.selectOptions(
    screen.getByRole("combobox", { name: `Status: ${PROJECT_TASK}` }),
    "investigating",
  );
  await user.type(
    screen.getByRole("textbox", { name: "New action item" }),
    "Recalculate the benefit-cost ratio",
  );
  await user.click(screen.getByRole("button", { name: "Add action item" }));
  expect(
    await screen.findByText("Recalculate the benefit-cost ratio"),
  ).toBeVisible();
  await next(user, "Next: conversations");
  await next(user, "Back");
  expect(
    screen.getByRole("combobox", { name: `Status: ${PROJECT_TASK}` }),
  ).toHaveValue("investigating");
  await next(user, "Next: conversations");
  await next(user, "Next: review findings");
  await user.click(screen.getByRole("button", { name: "Open report →" }));
  await next(user, "Explore the report");
  await user.click(screen.getByRole("tab", { name: /^Findings/ }));
  expect(
    await within(
      screen.getByRole("tabpanel", { name: /^Findings/ }),
    ).findByText(/The \$2.30 return does not follow/),
  ).toBeVisible();
  expect(invoke).not.toHaveBeenCalled();
  expect(navigate).not.toHaveBeenCalled();
  expect({ ...localStorage }).toEqual(stored);
  navigate.mockRestore();
}, 15000);

it("reopens the correct panels when backtracking and resets sample changes on restart", async () => {
  const user = userEvent.setup();
  render(<ExampleProject onClose={() => {}} />);
  await next(user, "Skip to project");
  await next(user, "Next: attention");
  expect(
    screen.queryByRole("combobox", { name: "Stage of the paper" }),
  ).toBeNull();
  await next(user, "Next: documents");
  await reachNotes(user);
  await next(user, "Next: action items");
  await next(user, "Back");
  expect(screen.getByRole("button", { name: "All notes" })).toBeVisible();
  await next(user, "Back");
  expect(screen.getByRole("group", { name: "Project context" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Close panel" }));
  expect(screen.getByRole("button", { name: "Context" })).toBeVisible();
  await next(user, "Restart");
  await next(user, "Skip to project");
  expect(
    screen.queryByRole("combobox", { name: "Stage of the paper" }),
  ).toBeNull();
  expect(invoke).not.toHaveBeenCalled();
});

it("opens from Home through the tutorial host and restores the preserved page and focus", async () => {
  const user = userEvent.setup();
  saveGetStarted({ dismissedAt: "2026-11-23" });
  const createProject = vi.fn();
  function Host() {
    const [draft, setDraft] = useState("Unfinished project work");
    return (
      <ExampleReviewSurfaceProvider>
        <TutorialRail />
        <ExampleReviewSurface>
          <HomePage
            projects={[]}
            projectsLoading={false}
            hasCurrentReview={false}
            reviewRunning={false}
            onAssistant={() => {}}
            onNewProject={createProject}
            onOpenProject={() => {}}
            onNewReview={() => {}}
            onContinueReview={() => {}}
            onNavigate={() => {}}
            onExampleProject={() => openGetStarted("project-example")}
          />
          <input
            aria-label="Live draft"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
        </ExampleReviewSurface>
        <GetStartedHost
          dependencies={null}
          dependenciesLoading={false}
          onCheckDependencies={() => {}}
          onNewReview={() => {}}
          onNewProject={createProject}
        />
      </ExampleReviewSurfaceProvider>
    );
  }
  render(<Host />);
  const opener = screen.getByRole("button", {
    name: "Explore an example project",
  });
  const draft = screen.getByRole("textbox", { name: "Live draft" });
  await user.click(opener);
  await screen.findByRole("dialog", { name: "Example project" });
  expect(draft).not.toBeVisible();
  await next(user, "Skip to project");
  const shortcut = vi.fn();
  window.addEventListener("keydown", shortcut);
  fireEvent.keyDown(document, { key: "n", ctrlKey: true });
  expect(shortcut).not.toHaveBeenCalled();
  await user.keyboard("{Escape}");
  window.removeEventListener("keydown", shortcut);
  await waitFor(() => expect(opener).toHaveFocus());
  expect(screen.getByRole("textbox", { name: "Live draft" })).toBe(draft);
  expect(draft).toHaveValue("Unfinished project work");
  expect(draft.closest("[inert]")).toBeNull();
  expect(createProject).not.toHaveBeenCalled();
  expect(invoke).not.toHaveBeenCalled();
});
