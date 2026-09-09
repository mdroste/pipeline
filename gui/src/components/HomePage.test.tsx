import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import HomePage from "./HomePage";

function renderHome(overrides: Partial<Parameters<typeof HomePage>[0]> = {}) {
  const props: Parameters<typeof HomePage>[0] = {
    projects: [],
    projectsLoading: false,
    hasCurrentReview: false,
    reviewRunning: false,
    onAssistant: vi.fn(),
    onNewProject: vi.fn(),
    onOpenProject: vi.fn(),
    onNewReview: vi.fn(),
    onContinueReview: vi.fn(),
    onNavigate: vi.fn(),
    ...overrides,
  };
  render(<HomePage {...props} />);
  return props;
}

it("offers outcome-first entry points without exposing runtime terminology", async () => {
  const user = userEvent.setup();
  const props = renderHome();

  expect(
    screen.getByRole("heading", { name: "Move your research forward." }),
  ).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: /Work with the assistant/ }),
  );
  await user.click(screen.getByRole("button", { name: /Review a paper/ }));
  await user.click(screen.getByRole("button", { name: /Automate research/ }));

  expect(props.onAssistant).toHaveBeenCalledOnce();
  expect(props.onNewReview).toHaveBeenCalledOnce();
  expect(props.onNavigate).toHaveBeenCalledWith("tasks");
  expect(
    screen.queryByText("workbench", { exact: false }),
  ).not.toBeInTheDocument();
});

it("shows recent projects and opens project creation", async () => {
  const user = userEvent.setup();
  const props = renderHome({
    projects: [
      {
        id: "monetary",
        name: "Monetary policy",
        root: "/papers/monetary",
        missingRootAt: null,
        updatedAt: "2026-09-08",
      },
      {
        id: "trade",
        name: "Trade paper",
        root: "/papers/trade",
        missingRootAt: "2026-09-08",
        updatedAt: "2026-09-07",
      },
    ],
  });

  await user.click(screen.getByRole("button", { name: /Monetary policy/ }));
  await user.click(screen.getByRole("button", { name: "New project" }));

  expect(props.onOpenProject).toHaveBeenCalledWith("monetary");
  expect(props.onNewProject).toHaveBeenCalledOnce();
  expect(screen.getByText("Folder unavailable")).toBeVisible();
});

it("keeps an active review easy to resume from Home", async () => {
  const user = userEvent.setup();
  const props = renderHome({ hasCurrentReview: true, reviewRunning: true });

  await user.click(screen.getByRole("button", { name: /A review is running/ }));
  expect(props.onContinueReview).toHaveBeenCalledOnce();
});
