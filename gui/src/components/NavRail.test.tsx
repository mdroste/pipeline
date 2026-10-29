import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import NavRail, { NAV_RAIL_WIDTH } from "./NavRail";
import { ReviewRunBeacon } from "./ReviewRunList";
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

function renderRail(overrides: Partial<ComponentProps<typeof NavRail>> = {}) {
  const props: ComponentProps<typeof NavRail> = {
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
  render(<NavRail {...props} />);
  return props;
}

describe("NavRail", () => {
  it("keeps every destination visible in one flat rail", async () => {
    const user = userEvent.setup();
    const props = renderRail();
    const reviews = within(screen.getByRole("region", { name: "Reviews" }));

    expect(screen.getByRole("button", { name: "Home" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByRole("button", { name: "System ready" })).toBeVisible();
    expect(
      screen.queryByRole("button", { name: /Tools/ }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Projects" }));
    expect(props.onNavigate).toHaveBeenCalledWith("project-index");
    await user.click(screen.getByRole("button", { name: "Automations" }));
    expect(props.onNavigate).toHaveBeenCalledWith("tasks");
    await user.click(reviews.getByRole("button", { name: "Review history" }));
    expect(props.onNavigate).toHaveBeenCalledWith("history");
    // Collections live inside history and project Reviews, not the rail.
    expect(
      reviews.queryByRole("button", { name: "Review collections" }),
    ).not.toBeInTheDocument();
    await user.click(reviews.getByRole("button", { name: "Review designer" }));
    expect(props.onNavigate).toHaveBeenCalledWith("pipeline");
    await user.click(reviews.getByRole("button", { name: "New review" }));
    expect(props.onNewRun).toHaveBeenCalledOnce();
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

    await user.click(screen.getByRole("button", { name: "Monetary policy" }));
    expect(onOpenProject).toHaveBeenCalledWith("monetary");
    expect(screen.getByRole("button", { name: /Trade paper/ })).toHaveAttribute(
      "title",
      "Trade paper · Folder unavailable",
    );
    expect(screen.getByLabelText("Folder unavailable")).toBeVisible();
  });

  it("shows project activity and attention without changing project navigation", () => {
    renderRail({ activePage: "workspace", workspaceActive: true });
    expect(screen.getByRole("button", { name: /^Projects/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(screen.getByLabelText("running")).toBeVisible();

    renderRail({ workspaceAttention: true });
    expect(screen.getByLabelText("needs attention")).toBeVisible();
  });

  it("marks the gallery as part of the designer destination", () => {
    renderRail({ activePage: "gallery" });

    expect(
      screen.getByRole("button", { name: "Review designer" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("keeps New review available and lists every queued, running, and finished review", async () => {
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

    expect(screen.getByRole("button", { name: "New review" })).toBeEnabled();
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
    const activeRow = list.getByRole("button", { name: /^active\.pdf/ });
    expect(activeRow).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("button", { name: "Review history" }),
    ).not.toHaveAttribute("aria-current");

    await user.click(list.getByRole("button", { name: /^finished\.pdf/ }));
    expect(onOpenRun).toHaveBeenCalledWith("finished");
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
  });

  it("shows no run list until a review is queued", () => {
    renderRail({ onOpenRun: vi.fn() });
    expect(
      screen.queryByRole("list", { name: /Reviews in progress/ }),
    ).toBeNull();
  });

  it("marks history active for a batch without adding a batch destination", () => {
    renderRail({ activePage: "batch" });

    expect(
      screen.getByRole("button", { name: "Review history" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.queryByRole("button", { name: /Current batch/ }),
    ).not.toBeInTheDocument();
  });

  it("opens dependency details from the system status", async () => {
    const user = userEvent.setup();
    const props = renderRail({
      dependenciesReady: false,
    });

    await user.click(screen.getByRole("button", { name: "Setup needed" }));
    expect(props.onDependencies).toHaveBeenCalledOnce();
  });

  it("exposes an adjustable primary-navigation divider", async () => {
    const user = userEvent.setup();
    const props = renderRail();
    const divider = screen.getByRole("separator", {
      name: "Resize primary navigation",
    });

    expect(divider).toHaveAttribute("aria-valuenow", "216");
    await user.click(divider);
    await user.keyboard("{ArrowRight}");
    expect(props.onResize).toHaveBeenCalledWith(224);
  });
});

describe("ReviewRunBeacon", () => {
  it("summarizes active and finished reviews and reveals the list", async () => {
    const user = userEvent.setup();
    const onReveal = vi.fn();
    const { rerender } = render(
      <ReviewRunBeacon
        runs={[run("a", "running"), run("b", "running"), run("c", "queued")]}
        unseen={new Set()}
        onReveal={onReveal}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: /^2 reviews running, 1 queued/ }),
    );
    expect(onReveal).toHaveBeenCalledOnce();

    rerender(
      <ReviewRunBeacon
        runs={[run("d", "done")]}
        unseen={new Set(["d"])}
        onReveal={onReveal}
      />,
    );
    expect(
      screen.getByRole("button", { name: /^1 review finished/ }),
    ).toBeVisible();

    rerender(
      <ReviewRunBeacon
        runs={[run("d", "done")]}
        unseen={new Set()}
        onReveal={onReveal}
      />,
    );
    expect(screen.queryByRole("button")).toBeNull();
  });
});
