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

    expect(screen.getByRole("button", { name: "New run" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.queryByRole("button", { name: /Current run/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "System ready" })).toBeVisible();

    await user.click(screen.getByRole("button", { name: "History" }));
    expect(props.onNavigate).toHaveBeenCalledWith("history");
  });

  it("adds a current-run destination and locks new runs while executing", () => {
    renderRail({
      hasCurrentRun: true,
      runInProgress: true,
    });

    expect(screen.getByRole("button", { name: "New run" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: /Current run/ }),
    ).toHaveAttribute("aria-current", "page");
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
