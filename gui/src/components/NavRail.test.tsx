import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import NavRail from "./NavRail";

function renderRail(
  overrides: Partial<ComponentProps<typeof NavRail>> = {},
) {
  const props: ComponentProps<typeof NavRail> = {
    activePage: "main",
    hasCurrentRun: false,
    hasActiveBatch: false,
    runInProgress: false,
    isMac: false,
    dependenciesReady: true,
    dependenciesLoading: false,
    width: 176,
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
  it("exposes labeled primary navigation and its active destination", async () => {
    const user = userEvent.setup();
    const props = renderRail();

    expect(screen.getByRole("button", { name: "New report" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.queryByRole("button", { name: /Current run/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "System ready" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "History" }));
    expect(props.onNavigate).toHaveBeenCalledWith("history");
    await user.click(screen.getByRole("button", { name: "Projects" }));
    expect(props.onNavigate).toHaveBeenCalledWith("projects");
    await user.click(screen.getByRole("button", { name: "Gallery" }));
    expect(props.onNavigate).toHaveBeenCalledWith("gallery");
  });

  it("adds a current-run destination and locks new runs while executing", () => {
    renderRail({
      hasCurrentRun: true,
      runInProgress: true,
    });

    expect(screen.getByRole("button", { name: "New report" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /Current report/ }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("keeps an active batch reachable while locking new runs", async () => {
    const user = userEvent.setup();
    const props = renderRail({
      activePage: "history",
      hasActiveBatch: true,
      runInProgress: true,
    });

    expect(screen.getByRole("button", { name: "New report" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: /Current batch/ }));
    expect(props.onNavigate).toHaveBeenCalledWith("batch");
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

    expect(divider).toHaveAttribute("aria-valuenow", "176");
    await user.click(divider);
    await user.keyboard("{ArrowRight}");
    expect(props.onResize).toHaveBeenCalledWith(184);
  });
});
