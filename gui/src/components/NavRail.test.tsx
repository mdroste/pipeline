import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import NavRail, { NAV_RAIL_WIDTH } from "./NavRail";

function renderRail(overrides: Partial<ComponentProps<typeof NavRail>> = {}) {
  const props: ComponentProps<typeof NavRail> = {
    activePage: "home",
    hasCurrentRun: false,
    runInProgress: false,
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

  it("locks new reviews without adding a current-run destination", () => {
    renderRail({
      activePage: "main",
      hasCurrentRun: true,
      runInProgress: true,
    });

    expect(screen.getByRole("button", { name: "New review" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Review history" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.queryByRole("button", { name: /Current run/ }),
    ).not.toBeInTheDocument();
  });

  it("marks history active for a batch without adding a batch destination", () => {
    renderRail({
      activePage: "batch",
      runInProgress: true,
    });

    expect(screen.getByRole("button", { name: "New review" })).toBeDisabled();
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
