import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ErrorBoundary from "./ErrorBoundary";

function Boom({ fail }: { fail: boolean }): JSX.Element {
  if (fail) throw new Error("kaboom");
  return <div>child-ok</div>;
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

  it("renders children when no error", () => {
    render(
      <ErrorBoundary>
        <Boom fail={false} />
      </ErrorBoundary>
    );
    expect(screen.getByText("child-ok")).toBeInTheDocument();
  });

  it("renders fallback UI with error message when a child throws", () => {
    render(
      <ErrorBoundary>
        <Boom fail={true} />
      </ErrorBoundary>
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();
    expect(screen.getByText(/kaboom/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
  });

  it("clears the error and remounts children when Try Again is clicked", async () => {
    const user = userEvent.setup();
    const { rerender } = render(
      <ErrorBoundary>
        <Boom fail={true} />
      </ErrorBoundary>
    );
    expect(screen.getByText("Something went wrong")).toBeInTheDocument();

    // Swap child to a non-throwing version, then click retry.
    rerender(
      <ErrorBoundary>
        <Boom fail={false} />
      </ErrorBoundary>
    );
    await user.click(screen.getByRole("button", { name: /try again/i }));
    expect(screen.getByText("child-ok")).toBeInTheDocument();
    expect(screen.queryByText("Something went wrong")).not.toBeInTheDocument();
  });
});
