import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
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
      deps: [dep({ name: "claude", authenticated: true, cli_auth_status: "signed_in" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("All dependencies found.")).toBeInTheDocument();
    expect(screen.getByText("signed in")).toBeInTheDocument();
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
      deps: [dep({ name: "claude", authenticated: false, cli_auth_status: "signed_out" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.getByText(/Required CLI not signed in\./)).toBeInTheDocument();
  });

  it("warns without blocking when Gemini OAuth cannot be probed noninteractively", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({
        name: "Gemini CLI",
        authenticated: undefined,
        cli_auth_status: "unknown",
        hint: "Gemini CLI authentication cannot be verified noninteractively.",
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("sign-in not verified")).toBeInTheDocument();
    expect(screen.queryByText(/Required CLI sign-in could not be verified\./)).not.toBeInTheDocument();
    expect(screen.getByText(/cannot be verified noninteractively/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("reports a signed-out CLI without blocking a configured direct API", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({
        authenticated: true,
        cli_auth_status: "signed_out",
        hint: "API key configured — CLI not required.",
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.queryByText(/Required CLI not signed in/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("treats missing optional deps as non-blocking ('Continue' button, no blocker text)", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "pdftoppm", found: false, required: false, version: "", path: "" })],
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

  it("is a labeled modal with refresh, Escape, and managed initial focus", async () => {
    const onDismiss = vi.fn();
    const onRefresh = vi.fn();
    const report: DepsReport = {
      ready: true,
      deps: [dep({ authenticated: true })],
    };
    const user = userEvent.setup();
    render(
      <DepsCheck
        report={report}
        onDismiss={onDismiss}
        onRefresh={onRefresh}
      />,
    );
    expect(screen.getByRole("dialog", { name: "Dependencies" })).toHaveAttribute(
      "aria-modal",
      "true",
    );
    const close = screen.getByRole("button", { name: "Close" });
    await waitFor(() => expect(close).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onRefresh).toHaveBeenCalledOnce();
    await user.keyboard("{Escape}");
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
