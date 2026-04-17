import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import DepsCheck from "./DepsCheck";
import type { DepStatus, DepsReport } from "../lib/types";

function dep(overrides: Partial<DepStatus>): DepStatus {
  return {
    name: "claude",
    found: true,
    version: "1.0.0",
    path: "/usr/local/bin/claude",
    required: true,
    hint: "install it",
    ...overrides,
  };
}

describe("DepsCheck", () => {
  it("shows 'All dependencies found' when everything is installed and authenticated", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "claude", authenticated: true })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("All dependencies found.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("shows 'required' badge and blocker message for a missing required dep", () => {
    const report: DepsReport = {
      ready: false,
      deps: [dep({ name: "claude", found: false, required: true, version: "", path: "" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("required")).toBeInTheDocument();
    expect(screen.getByText(/Required dependencies missing\./)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("shows 'not signed in' for a required dep that is present but unauthenticated", () => {
    const report: DepsReport = {
      ready: false,
      deps: [dep({ name: "claude", authenticated: false })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.getByText(/Required CLI not signed in\./)).toBeInTheDocument();
  });

  it("treats missing optional deps as non-blocking ('Continue' button, no blocker text)", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "marker_single", found: false, required: false, version: "", path: "" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("optional")).toBeInTheDocument();
    expect(screen.queryByText(/Required dependencies missing/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("invokes onDismiss when the action button is clicked", async () => {
    const onDismiss = vi.fn();
    const report: DepsReport = {
      ready: true,
      deps: [dep({ authenticated: true })],
    };
    render(<DepsCheck report={report} onDismiss={onDismiss} />);
    await userEvent.setup().click(screen.getByRole("button"));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
