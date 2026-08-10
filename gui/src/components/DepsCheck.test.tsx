import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import DepsCheck from "./DepsCheck";
import type { DepStatus, DepsReport } from "../lib/types";

function dep(overrides: Partial<DepStatus>): DepStatus {
  return {
    name: "Claude CLI",
    found: true,
    version: "1.0.0",
    path: "/usr/local/bin/claude",
    required: true,
    hint: "install it",
    ...overrides,
  };
}

describe("DepsCheck", () => {
  it("shows separate ready sections when model access and PDF parsing are available", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "Claude CLI", authenticated: true, cli_auth_status: "signed_in" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByRole("region", { name: "Model access" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "PDF parsing" })).toBeInTheDocument();
    expect(screen.getByText(/Use at least one: GPT \(Codex\), Claude, or Gemini CLI/)).toBeInTheDocument();
    expect(screen.getByText("signed in")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("shows a model-access blocker when no provider is usable", () => {
    const report: DepsReport = {
      ready: false,
      deps: [dep({ name: "Claude CLI", found: false, required: true, version: "", path: "" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("set up")).toBeInTheDocument();
    expect(screen.getByText(/Set up at least one model provider/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("accepts any usable model provider without failing the dependency check", () => {
    const report: DepsReport = {
      ready: true,
      deps: [
        dep({ name: "Claude CLI", found: false, required: true, version: "", path: "" }),
        dep({ name: "Codex CLI", authenticated: true, required: false }),
        dep({ name: "Gemini CLI", found: false, required: false, version: "", path: "" }),
      ],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    const models = screen.getByRole("region", { name: "Model access" });
    expect(within(models).getByText("Ready")).toBeInTheDocument();
    expect(within(models).getAllByText("alternative")).toHaveLength(2);
    expect(screen.queryByText(/Set up at least one model provider/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("links a missing required PaddleOCR-VL extractor to PDF Extraction settings", async () => {
    const onOpenPdfSettings = vi.fn();
    const report: DepsReport = {
      ready: false,
      deps: [dep({
        name: "PaddleOCR-VL Full Parser",
        found: false,
        required: true,
        version: "",
        path: "",
        hint: "Recommended for PDFs: Install from Settings → PDF Extraction.",
      })],
    };
    render(
      <DepsCheck
        report={report}
        onDismiss={() => {}}
        onOpenPdfSettings={onOpenPdfSettings}
      />,
    );
    const label = screen.getByText("highly recommended (for PDFs)");
    expect(label).toHaveClass("text-red-700");
    expect(screen.queryByText("required")).not.toBeInTheDocument();
    expect(screen.getByText(/Required PDF parsing tools are missing\./)).toBeInTheDocument();
    const settingsLink = screen.getByRole("link", {
      name: "Settings → PDF Extraction",
    });
    expect(settingsLink).toHaveAttribute("href", "#paddleocr-local-engine");
    await userEvent.setup().click(settingsLink);
    expect(onOpenPdfSettings).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("shows 'not signed in' for a required dep that is present but unauthenticated", () => {
    const report: DepsReport = {
      ready: false,
      deps: [dep({ name: "Claude CLI", authenticated: false, cli_auth_status: "signed_out" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.getByText(/Set up at least one model provider/)).toBeInTheDocument();
  });

  it("uses the same signed-out label when Gemini sign-in cannot be verified", () => {
    const hint = "Gemini CLI is installed, but sign-in could not be verified.";
    const report: DepsReport = {
      ready: false,
      deps: [dep({
        name: "Gemini CLI",
        authenticated: undefined,
        cli_auth_status: "unknown",
        hint,
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.queryByText("sign-in not verified")).not.toBeInTheDocument();
    expect(screen.getByText(hint)).toBeInTheDocument();
    expect(screen.getByText(/Set up at least one model provider/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("does not show local LLM or PDF extractor configuration cards", () => {
    const report: DepsReport = {
      ready: true,
      deps: [
        dep({ name: "Claude CLI", authenticated: true }),
        dep({ name: "Local LLM server", required: false }),
        dep({ name: "PDF extractor configuration", required: false }),
      ],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("Claude CLI")).toBeInTheDocument();
    expect(screen.queryByText("Local LLM server")).not.toBeInTheDocument();
    expect(screen.queryByText("PDF extractor configuration")).not.toBeInTheDocument();
  });

  it("shows a configured API key as ready even when the CLI is signed out", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({
        authenticated: true,
        cli_auth_status: "signed_out",
        hint: "API key configured — CLI not required.",
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("API key configured")).toBeInTheDocument();
    expect(screen.queryByText("not signed in")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("treats missing optional PDF tools as non-blocking", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "pdftoppm", found: false, required: false, version: "", path: "" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("optional")).toBeInTheDocument();
    expect(screen.queryByText(/Required PDF parsing tools are missing/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
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

  it("is a labeled modal with Escape and managed initial focus", async () => {
    const onDismiss = vi.fn();
    const report: DepsReport = {
      ready: true,
      deps: [dep({ authenticated: true })],
    };
    const user = userEvent.setup();
    render(<DepsCheck report={report} onDismiss={onDismiss} />);
    expect(screen.getByRole("dialog", { name: "Dependencies" })).toHaveAttribute(
      "aria-modal",
      "true",
    );
    const close = screen.getByRole("button", { name: "Close" });
    await waitFor(() => expect(close).toHaveFocus());
    expect(screen.queryByRole("button", { name: "Refresh" })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
