import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import DepsCheck from "./DepsCheck";
import type { DepStatus, DepsReport } from "../lib/types";

const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

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
  it("shows the managed Workflow sign-in as ready without CLI authentication", () => {
    render(<DepsCheck report={{ ready: true, deps: [dep({
      name: "Workflow ChatGPT", authenticated: true, cli_auth_status: undefined,
    })] }} onDismiss={() => {}} />);
    const models = screen.getByRole("region", { name: "Model access" });
    expect(within(models).getByText("Reviews ChatGPT")).toBeVisible();
    expect(within(models).getByText("signed in")).toBeVisible();
    expect(within(models).getByText("Ready")).toBeVisible();
  });

  it("highlights a required connection failure even when another provider is ready", () => {
    const hint = "Workflow ChatGPT connection is in use by another Pipeline process";
    render(<DepsCheck report={{ ready: false, deps: [
      dep({ authenticated: true, required: false }),
      dep({ name: "Workflow ChatGPT", authenticated: false, hint }),
      dep({ name: "Antigravity CLI", found: false, required: false }),
    ] }} onDismiss={() => {}} />);
    const models = screen.getByRole("region", { name: "Model access" });
    expect(within(models).getByText("Needs attention")).toBeVisible();
    expect(within(models).getByText("set up")).toHaveClass("text-red-700");
    expect(within(models).getByText("alternative")).toHaveClass("text-gray-600");
    expect(within(models).getByText("Reviews ChatGPT connection is in use by another Pipeline process")).toBeVisible();
  });

  it("includes hidden required blockers in their section and shows their remedy", () => {
    render(<DepsCheck report={{ ready: false, deps: [
      dep({ name: "Local LLM server", found: false, hint: "Start the local server." }),
      dep({ name: "PDF extractor configuration", found: false, hint: "Choose a supported extractor." }),
    ] }} onDismiss={() => {}} />);
    const models = screen.getByRole("region", { name: "Model access" });
    const pdf = screen.getByRole("region", { name: "PDF parsing" });
    expect(within(models).getByText("Needs attention")).toBeVisible();
    expect(within(models).getByText("Start the local server.")).toBeVisible();
    expect(within(pdf).getByText("Needs attention")).toBeVisible();
    expect(within(pdf).getByText("Choose a supported extractor.")).toBeVisible();
  });

  it("opens an official platform installation guide in the system browser", async () => {
    openUrl.mockResolvedValueOnce(undefined);
    const report: DepsReport = {
      ready: false,
      deps: [dep({
        found: false,
        version: "",
        path: "",
        hint: "Install Claude Code using the official Windows instructions.",
        help_url: "https://code.claude.com/docs/en/installation",
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    const link = screen.getByRole("link", { name: "Official installation guide" });
    expect(link).toHaveAttribute("href", "https://code.claude.com/docs/en/installation");
    await userEvent.setup().click(link);
    expect(openUrl).toHaveBeenCalledWith("https://code.claude.com/docs/en/installation");
  });

  it("shows separate ready sections when model access and PDF parsing are available", () => {
    const report: DepsReport = {
      ready: true,
      deps: [dep({ name: "Claude CLI", authenticated: true, cli_auth_status: "signed_in" })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByRole("region", { name: "Model access" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "PDF parsing" })).toBeInTheDocument();
    expect(
      screen.getByText(/Each provider selected by this workflow must be ready/),
    ).toBeInTheDocument();
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
    expect(screen.getByText(/Check the required model connections/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("keeps unused model providers optional when the required provider is ready", () => {
    const report: DepsReport = {
      ready: true,
      deps: [
        dep({ name: "Claude CLI", found: false, required: false, version: "", path: "" }),
        dep({ name: "Codex CLI", authenticated: true, required: true }),
        dep({ name: "Antigravity CLI", found: false, required: false, version: "", path: "" }),
      ],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    const models = screen.getByRole("region", { name: "Model access" });
    expect(within(models).getByText("Ready")).toBeInTheDocument();
    expect(within(models).getAllByText("alternative")).toHaveLength(2);
    expect(screen.queryByText(/Check the required model connections/)).not.toBeInTheDocument();
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
      name: "Settings → Reviews → PDF Extraction",
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
    expect(screen.getByText(/Check the required model connections/)).toBeInTheDocument();
  });

  it("distinguishes an unverified sign-in from a signed-out account", () => {
    const hint =
      "Antigravity CLI is installed, but sign-in could not be verified. Run `agy` in a terminal to sign in, then refresh this check.";
    const report: DepsReport = {
      ready: false,
      deps: [dep({
        name: "Antigravity CLI",
        authenticated: undefined,
        cli_auth_status: "unknown",
        hint,
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("sign-in not verified")).toBeInTheDocument();
    expect(screen.queryByText("not signed in")).not.toBeInTheDocument();
    expect(screen.getByText(hint)).toBeInTheDocument();
    expect(screen.getByText(/Check the required model connections/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dismiss" })).toBeInTheDocument();
  });

  it("surfaces the deterministic Antigravity signed-out state with the agy sign-in hint", () => {
    const hint =
      "Antigravity CLI is installed but not signed in. Run `agy` in a terminal to sign in, then refresh this check.";
    const report: DepsReport = {
      ready: false,
      deps: [dep({
        name: "Antigravity CLI",
        required: true,
        authenticated: false,
        cli_auth_status: "signed_out",
        version: "1.1.12",
        hint,
      })],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("not signed in")).toBeInTheDocument();
    expect(screen.getByText("1.1.12")).toBeInTheDocument();
    expect(screen.getByText(hint)).toBeInTheDocument();
    expect(screen.getByText(/Check the required model connections/)).toBeInTheDocument();
  });

  it("hides internal and bundled dependency cards", () => {
    const report: DepsReport = {
      ready: true,
      deps: [
        dep({ name: "Claude CLI", authenticated: true }),
        dep({ name: "Local LLM server", required: false }),
        dep({ name: "PDF extractor configuration", required: false }),
        dep({ name: "pdftoppm", required: false }),
        dep({ name: "pdftotext", required: false }),
      ],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.getByText("Claude CLI")).toBeInTheDocument();
    expect(screen.queryByText("Local LLM server")).not.toBeInTheDocument();
    expect(screen.queryByText("PDF extractor configuration")).not.toBeInTheDocument();
    expect(screen.queryByText("pdftoppm")).not.toBeInTheDocument();
    expect(screen.queryByText("pdftotext")).not.toBeInTheDocument();
  });

  it("reports a missing required bundled PDF tool without exposing its executable", () => {
    const report: DepsReport = {
      ready: false,
      deps: [
        dep({ name: "Claude CLI", authenticated: true }),
        dep({ name: "pdftotext", found: false, required: true, version: "", path: "" }),
      ],
    };
    render(<DepsCheck report={report} onDismiss={() => {}} />);
    expect(screen.queryByText("pdftotext")).not.toBeInTheDocument();
    expect(screen.getByText("Bundled PDF tools are unavailable. Reinstall Pipeline.")).toBeInTheDocument();
    const pdfSection = screen.getByRole("region", { name: "PDF parsing" });
    expect(within(pdfSection).getByText("Needs attention")).toBeInTheDocument();
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
      deps: [dep({
        name: "PaddleOCR-VL Full Parser",
        found: false,
        required: false,
        version: "",
        path: "",
      })],
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
