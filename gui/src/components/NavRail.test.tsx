import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useLayoutEffect, type ComponentProps } from "react";
import { createPortal } from "react-dom";
import NavRail, { CollapsedNavBar, NAV_RAIL_WIDTH } from "./NavRail";
import { NavRailSlotProvider, useNavRailSlot } from "./nav-rail/navRailSlot";
import { queuedRun } from "../test/queuedRunFixture";
import type { QueuedRun } from "../lib/types";

// Times relative to now so the rail's relative labels are stable.
function run(
  key: string,
  status: QueuedRun["status"],
  extra: Partial<QueuedRun> = {},
): QueuedRun {
  const now = Date.now();
  const finished = status !== "running" && status !== "queued";
  return queuedRun(key, status, {
    stage: status === "running" ? "dispatching" : null,
    startedAt: status === "queued" ? null : now - 125_000,
    finishedAt: finished ? now - 30_000 : null,
    ...extra,
  });
}

function railProps(
  overrides: Partial<ComponentProps<typeof NavRail>> = {},
): ComponentProps<typeof NavRail> {
  return {
    activePage: "home",
    hasCurrentRun: false,
    isMac: false,
    dependenciesReady: true,
    dependenciesLoading: false,
    width: NAV_RAIL_WIDTH.default,
    onResize: vi.fn(),
    onNewRun: vi.fn(),
    onNavigate: vi.fn(),
    onDependencies: vi.fn(),
    ...overrides,
  };
}

function renderRail(overrides: Partial<ComponentProps<typeof NavRail>> = {}) {
  const props = railProps(overrides);
  render(<NavRail {...props} />);
  return props;
}

const expand = (user: ReturnType<typeof userEvent.setup>, section: string) =>
  user.click(screen.getByRole("button", { name: `Expand ${section}` }));

describe("NavRail", () => {
  beforeEach(() => localStorage.clear());

  it("starts with the destinations and opens what each one holds", async () => {
    const user = userEvent.setup();
    const props = renderRail();

    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.queryByRole("button", { name: "History" })).toBeNull();
    expect(screen.getByRole("button", { name: "System ready" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Conversations" }));
    expect(props.onNavigate).toHaveBeenCalledWith("conversations");
    await user.click(screen.getByRole("button", { name: "Projects" }));
    expect(props.onNavigate).toHaveBeenCalledWith("project-index");
    await user.click(screen.getByRole("button", { name: "Automations" }));
    expect(props.onNavigate).toHaveBeenCalledWith("tasks");
    await user.click(screen.getByRole("button", { name: "Reviews" }));
    expect(props.onNavigate).toHaveBeenCalledWith("history");

    await expand(user, "Reviews");
    const reviews = within(screen.getByRole("group", { name: "Reviews" }));
    await user.click(reviews.getByRole("button", { name: "Designer" }));
    expect(props.onNavigate).toHaveBeenCalledWith("pipeline");
    await user.click(reviews.getByRole("button", { name: "New review" }));
    expect(props.onNewRun).toHaveBeenCalledOnce();
    // Collections live inside history and project Reviews, not the rail.
    expect(
      reviews.queryByRole("button", { name: "Review collections" }),
    ).not.toBeInTheDocument();
  });

  it("starts each kind of work from one menu", async () => {
    const user = userEvent.setup();
    const onNewConversation = vi.fn();
    const onNewProject = vi.fn();
    const props = renderRail({ onNewConversation, onNewProject });

    const choose = async (name: RegExp) => {
      await user.click(screen.getByRole("button", { name: "New" }));
      await user.click(screen.getByRole("menuitem", { name }));
    };
    await choose(/^New conversation/);
    expect(onNewConversation).toHaveBeenCalledOnce();
    await choose(/^New review/);
    expect(props.onNewRun).toHaveBeenCalledOnce();
    await choose(/^New project/);
    expect(onNewProject).toHaveBeenCalledOnce();
    await choose(/^New automation/);
    expect(props.onNavigate).toHaveBeenCalledWith("tasks");
  });

  it("opens the section holding the page and remembers a section left open", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <NavRail {...railProps({ activePage: "pipeline" })} />,
    );
    expect(screen.getByRole("button", { name: "Designer" })).toHaveAttribute(
      "aria-current",
      "page",
    );

    rerender(<NavRail {...railProps({ activePage: "home" })} />);
    expect(screen.queryByRole("button", { name: "Designer" })).toBeNull();

    await expand(user, "Reviews");
    rerender(<NavRail {...railProps({ activePage: "tasks" })} />);
    expect(screen.getByRole("button", { name: "Designer" })).toBeVisible();

    // Closing a section by hand does not stop a later visit from opening it.
    await user.click(screen.getByRole("button", { name: "Collapse Reviews" }));
    expect(screen.queryByRole("button", { name: "Designer" })).toBeNull();
    rerender(<NavRail {...railProps({ activePage: "history" })} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "History" })).toBeVisible(),
    );
  });

  it("opens recent projects directly and identifies unavailable folders", async () => {
    const user = userEvent.setup();
    const onOpenProject = vi.fn();
    renderRail({
      onOpenProject,
      recentProjects: [
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

    await expand(user, "Projects");
    await user.click(screen.getByRole("button", { name: "Monetary policy" }));
    expect(onOpenProject).toHaveBeenCalledWith("monetary");
    expect(screen.getByRole("button", { name: /Trade paper/ })).toHaveAttribute(
      "title",
      "Trade paper · Folder unavailable",
    );
    expect(screen.getByLabelText("Folder unavailable")).toBeVisible();
  });

  it("shows project activity and attention on the Projects row", () => {
    renderRail({ activePage: "project-index", workspaceActive: true });
    expect(screen.getByRole("button", { name: /^Projects/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByLabelText("running")).toBeVisible();

    renderRail({ workspaceAttention: true });
    expect(screen.getByLabelText("needs attention")).toBeVisible();
  });

  it("marks gallery and batch as parts of their review destinations", () => {
    const { unmount } = render(
      <NavRail {...railProps({ activePage: "gallery" })} />,
    );
    expect(screen.getByRole("button", { name: "Designer" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    unmount();

    render(<NavRail {...railProps({ activePage: "batch" })} />);
    expect(screen.getByRole("button", { name: "History" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.queryByRole("button", { name: /Current batch/ }),
    ).not.toBeInTheDocument();
  });

  it("lists every queued, running, and finished review in Activity", async () => {
    const user = userEvent.setup();
    const onOpenRun = vi.fn();
    const onCancelRun = vi.fn();
    const onDismissRuns = vi.fn();
    const finishedAt = Date.now() - 30_000;
    const runs = [
      run("finished", "done", { finishedAt }),
      run("waiting", "queued"),
      run("active", "running"),
      run("broken", "failed", { error: "provider exploded", finishedAt }),
    ];
    renderRail({
      activePage: "main",
      hasCurrentRun: true,
      runs,
      runLimit: 1,
      activeRunKey: "active",
      unseenRuns: new Set(["finished"]),
      onOpenRun,
      onCancelRun,
      onDismissRuns,
    });

    const activity = screen.getByRole("button", { name: /^Activity/ });
    expect(activity).toHaveTextContent("1 running · 1 for you");
    await user.click(activity);
    expect(screen.getByRole("status")).toHaveTextContent(
      "1 of 1 running · 1 queued",
    );
    const list = within(
      screen.getByRole("list", {
        name: "Reviews in progress and recently finished",
      }),
    );
    const rows = list.getAllByRole("listitem");
    // Running first, then queued, then finished.
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringMatching(/^active\.pdfReviewing · 2:0\d/),
      "waiting.pdfQueued · next",
      expect.stringMatching(
        /^finished\.pdfReport ready · just now · not opened yet/,
      ),
      "broken.pdfFailed",
    ]);
    expect(list.getByRole("button", { name: /^active\.pdf/ })).toHaveAttribute(
      "aria-current",
      "page",
    );

    await user.click(
      list.getByRole("button", { name: "Cancel review of active.pdf" }),
    );
    expect(onCancelRun).toHaveBeenCalledWith(runs[2]);
    await user.click(
      list.getByRole("button", { name: "Remove waiting.pdf from the queue" }),
    );
    expect(onCancelRun).toHaveBeenCalledWith(runs[1]);
    await user.click(list.getByRole("button", { name: "Dismiss broken.pdf" }));
    expect(onDismissRuns).toHaveBeenCalledWith("broken");
    await user.click(
      screen.getByRole("button", { name: "Clear finished (2)" }),
    );
    expect(onDismissRuns).toHaveBeenLastCalledWith(undefined);

    await user.click(list.getByRole("button", { name: /^finished\.pdf/ }));
    expect(onOpenRun).toHaveBeenCalledWith("finished");
    expect(screen.queryByRole("dialog", { name: "Activity" })).toBeNull();
  });

  it("collects conversation and automation attention in Activity", async () => {
    const user = userEvent.setup();
    const onOpenConversations = vi.fn();
    const onOpenAutomations = vi.fn();
    const onActivity = vi.fn();
    renderRail({
      workspaceAttention: true,
      tasksAttention: true,
      onOpenConversations,
      onOpenAutomations,
      onActivity,
    });
    const open = () =>
      user.click(screen.getByRole("button", { name: /^Activity/ }));

    await open();
    await user.click(
      screen.getByRole("button", { name: /conversation needs your approval/ }),
    );
    expect(onOpenConversations).toHaveBeenCalledOnce();
    await open();
    await user.click(
      screen.getByRole("button", { name: /automation has an update/ }),
    );
    expect(onOpenAutomations).toHaveBeenCalledOnce();
    await open();
    await user.click(screen.getByRole("button", { name: "See all activity" }));
    expect(onActivity).toHaveBeenCalledOnce();
  });

  it("says so when nothing is running", async () => {
    const user = userEvent.setup();
    renderRail({ onOpenRun: vi.fn() });
    await user.click(screen.getByRole("button", { name: "Activity" }));
    expect(screen.getByText("Nothing is running.")).toBeVisible();
    expect(
      screen.queryByRole("list", { name: /Reviews in progress/ }),
    ).toBeNull();
  });

  it("shows a setup row only when the system needs something", async () => {
    const user = userEvent.setup();
    const props = renderRail({ dependenciesReady: false });

    await user.click(screen.getByRole("button", { name: "Setup needed" }));
    expect(props.onDependencies).toHaveBeenCalledOnce();
    expect(screen.getByText("See what is missing")).toBeVisible();
  });

  it("exposes an adjustable primary-navigation divider", async () => {
    const user = userEvent.setup();
    const props = renderRail();
    const divider = screen.getByRole("separator", {
      name: "Resize primary navigation",
    });

    expect(divider).toHaveAttribute("aria-valuenow", "240");
    await user.click(divider);
    await user.keyboard("{ArrowRight}");
    expect(props.onResize).toHaveBeenCalledWith(248);
  });

  it("offers to fold itself from a button at the top", async () => {
    const user = userEvent.setup();
    const onToggleCollapsed = vi.fn();
    renderRail({ onToggleCollapsed });

    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(onToggleCollapsed).toHaveBeenCalledOnce();
  });

  it("keeps every destination as an icon, with sections in a flyout", async () => {
    const user = userEvent.setup();
    const props = renderRail({ collapsed: true, tasksAttention: true });

    await user.click(screen.getByRole("button", { name: "Home" }));
    expect(props.onNavigate).toHaveBeenCalledWith("home");
    expect(screen.getByLabelText("Automation update")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Reviews" }));
    const flyout = within(screen.getByRole("dialog", { name: "Reviews" }));
    await user.click(flyout.getByRole("button", { name: "History" }));
    expect(props.onNavigate).toHaveBeenCalledWith("history");
    expect(screen.queryByRole("dialog", { name: "Reviews" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Reviews" }));
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Reviews" })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Reviews" }));
    await user.click(document.body);
    expect(screen.queryByRole("dialog", { name: "Reviews" })).toBeNull();

    expect(
      screen.queryByRole("separator", { name: "Resize primary navigation" }),
    ).toBeNull();
  });
});

describe("Workspace navigation in the sidebar", () => {
  function Reporter({ projectId }: { projectId: string | null }) {
    const rail = useNavRailSlot();
    const setWorkspace = rail?.setWorkspace;
    useLayoutEffect(() => {
      setWorkspace?.({
        projectId,
        projectName: projectId ? "Monetary policy" : null,
        newConversation,
        openView: vi.fn(),
      });
      return () => setWorkspace?.(null);
    }, [setWorkspace]);
    return rail?.slot
      ? createPortal(<button type="button">Project tree</button>, rail.slot)
      : null;
  }
  const newConversation = vi.fn();
  const renderWorkspace = (projectId: string | null) =>
    render(
      <NavRailSlotProvider>
        <NavRail {...railProps({ activePage: "workspace" })} />
        <Reporter projectId={projectId} />
      </NavRailSlotProvider>,
    );

  it("places an open project under Projects and starts conversations there", async () => {
    const user = userEvent.setup();
    renderWorkspace("monetary");

    const projects = within(screen.getByRole("group", { name: "Projects" }));
    expect(
      await projects.findByRole("button", { name: "Project tree" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "New" }));
    await user.click(
      screen.getByRole("menuitem", { name: /In Monetary policy/ }),
    );
    expect(newConversation).toHaveBeenCalledOnce();
  });

  it("places an unfiled conversation under Conversations", async () => {
    renderWorkspace(null);
    const conversations = within(
      screen.getByRole("group", { name: "Conversations" }),
    );
    expect(
      await conversations.findByRole("button", { name: "Project tree" }),
    ).toBeVisible();
    expect(screen.queryByRole("group", { name: "Projects" })).toBeNull();
  });
});

describe("CollapsedNavBar", () => {
  it("drags the window from the top of the rail and collapsed bar on Windows", () => {
    renderRail({ isWindows: true });
    expect(
      document.querySelector("aside > [data-tauri-drag-region]"),
    ).toBeInTheDocument();

    render(<CollapsedNavBar isMac={false} isWindows onExpand={vi.fn()} />);
    expect(screen.getByTestId("collapsed-nav-bar")).toHaveAttribute(
      "data-tauri-drag-region",
      "deep",
    );
  });

  it("leaves dragging to the native title bar on other platforms", () => {
    renderRail();
    expect(
      document.querySelector("[data-tauri-drag-region]"),
    ).not.toBeInTheDocument();
  });

  it("brings the full sidebar back", async () => {
    const user = userEvent.setup();
    const onExpand = vi.fn();
    render(<CollapsedNavBar isMac={false} onExpand={onExpand} />);

    await user.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(onExpand).toHaveBeenCalledOnce();
  });
});
