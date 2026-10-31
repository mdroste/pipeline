import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ErrorBoundary from "./ErrorBoundary";

const platform = vi.hoisted(() => ({ isMac: false, isWindows: false }));

vi.mock("../lib/platform", () => platform);
vi.mock("./WindowControls", () => ({
  default: () => <div data-testid="window-controls" />,
}));

function Boom({ fail }: { fail: boolean }) {
  if (fail) {
    const error = new Error("kaboom\n/private/project/secret.ts");
    error.stack = "Error: kaboom\n    at /private/project/secret.ts:42:7";
    throw error;
  }
  return <div>child-ok</div>;
}

function Clicker({ fail }: { fail: boolean }) {
  const [clicks, setClicks] = useState(0);
  if (fail) throw new Error("kaboom");
  return (
    <button onClick={() => setClicks((count) => count + 1)}>
      clicks {clicks}
    </button>
  );
}

describe("ErrorBoundary", () => {
  // React logs caught errors to console.error; suppress for cleaner test output.
  let errSpy: ReturnType<typeof vi.spyOn>;
  beforeEach(() => {
    errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
  });
  afterEach(() => {
    errSpy.mockRestore();
  });

  it("keeps the window controls on the app-wide screen on Windows", () => {
    platform.isWindows = true;
    try {
      const { container } = render(
        <ErrorBoundary>
          <Boom fail={true} />
        </ErrorBoundary>,
      );
      expect(screen.getByTestId("window-controls")).toBeInTheDocument();
      expect(
        container.querySelector("[data-tauri-drag-region]"),
      ).toBeInTheDocument();
    } finally {
      platform.isWindows = false;
    }
  });

  it("leaves the window controls to the shell inside a pane", () => {
    platform.isWindows = true;
    try {
      render(
        <ErrorBoundary nested>
          <Boom fail={true} />
        </ErrorBoundary>,
      );
      expect(screen.queryByTestId("window-controls")).not.toBeInTheDocument();
    } finally {
      platform.isWindows = false;
    }
  });

  it("renders children when no error", () => {
    render(
      <ErrorBoundary>
        <Boom fail={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("child-ok")).toBeInTheDocument();
  });

  it("renders fallback UI with error message when a child throws", () => {
    render(
      <ErrorBoundary>
        <Boom fail={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText("kaboom")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /try again/i }),
    ).toBeInTheDocument();
  });

  it("hides raw stack details in release-mode fallback UI", () => {
    render(
      <ErrorBoundary showTechnicalDetails={false}>
        <Boom fail={true} />
      </ErrorBoundary>,
    );

    expect(screen.getByText("kaboom")).toBeVisible();
    expect(
      screen.queryByText(/private\/project\/secret/),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Technical details")).not.toBeInTheDocument();
  });

  it("keeps stack details available in explicitly enabled development diagnostics", () => {
    render(
      <ErrorBoundary showTechnicalDetails>
        <Boom fail={true} />
      </ErrorBoundary>,
    );

    expect(screen.getByText("Technical details")).toBeInTheDocument();
    expect(screen.getByText(/private\/project\/secret/)).toBeInTheDocument();
  });

  it("clears the error and remounts children when Try Again is clicked", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <ErrorBoundary>
        <Boom fail={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();

    // Swap child to a non-throwing version, then click retry.
    rerender(
      <ErrorBoundary>
        <Boom fail={false} />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(screen.getByText("child-ok")).toBeInTheDocument();
    expect(screen.queryByText("Something went wrong")).not.toBeInTheDocument();
  });

  it("adds no layout box, so children keep the parent's height chain", () => {
    const { container } = render(
      <ErrorBoundary>
        <Boom fail={false} />
      </ErrorBoundary>,
    );
    expect(container.firstChild).toBe(screen.getByText("child-ok"));
  });

  it("remounts children with fresh state after Try Again", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <ErrorBoundary nested>
        <Clicker fail={false} />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: "clicks 0" }));
    expect(screen.getByRole("button", { name: "clicks 1" })).toBeVisible();

    rerender(
      <ErrorBoundary nested>
        <Clicker fail={true} />
      </ErrorBoundary>,
    );
    rerender(
      <ErrorBoundary nested>
        <Clicker fail={false} />
      </ErrorBoundary>,
    );
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(screen.getByRole("button", { name: "clicks 0" })).toBeVisible();
  });

  it("fills the window at the root and its pane when nested", () => {
    const { unmount } = render(
      <ErrorBoundary>
        <Boom fail={true} />
      </ErrorBoundary>,
    );
    expect(screen.getByRole("alert").parentElement).toHaveClass("h-screen");
    unmount();

    render(
      <ErrorBoundary nested>
        <Boom fail={true} />
      </ErrorBoundary>,
    );
    const fallback = screen.getByRole("alert").parentElement;
    expect(fallback).toHaveClass("h-full", "min-h-0", "overflow-auto");
    expect(fallback).not.toHaveClass("h-screen");
  });
});
