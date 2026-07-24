import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPage from "./SettingsPage";
import type { Settings } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));

function makeSettings(): Settings {
  return {
    preferred_provider: "claude",
    max_workers: 5,
    active_profile: "deep-review",
    claude_model: "",
    claude_effort: "",
    codex_model: "",
    codex_effort: "",
    gemini_model: "",
    pdf_extractor: "llm",
    marker_disable_ocr: false,
    marker_disable_images: true,
    verbose_logging: false,
    step_timeout_secs: 1200,
    max_retries: 1,
    auto_revision_reconciliation: false,
    max_saved_runs: 0,
    max_saved_run_bytes: 5_000_000_000,
    anthropic_api_key: "",
    openai_api_key: "",
    google_api_key: "",
    local_base_url: "http://localhost:11434/v1",
    local_model: "",
    local_api_key: "",
  };
}

function mockLoad(settings: Settings, warnings: string[] = []) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings") return Promise.resolve({ settings, warnings });
    if (cmd === "save_settings") return Promise.resolve();
    if (cmd === "list_engines") return Promise.resolve([]);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

describe("SettingsPage", () => {
  beforeEach(() => {
    invoke.mockReset();
  });

  it("loads settings and renders the LLM provider section", async () => {
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    expect(await screen.findByText("Preferred Provider")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Claude (Anthropic)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "ChatGPT (OpenAI)" })).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_settings");
  });

  it("offers PaddleOCR-VL as a PDF extraction method", async () => {
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "PDF Extraction" }));
    expect(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Q8/,
      }),
    ).toBeInTheDocument();
  });

  it("surfaces backend warnings from settings load", async () => {
    mockLoad(makeSettings(), [
      "Settings file has invalid JSON: oops. It was moved to settings.json.corrupt",
    ]);
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    expect(
      await screen.findByText(/settings\.json\.corrupt/),
    ).toBeInTheDocument();
  });

  it("saves settings and shows the saved indicator", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await screen.findByText("Preferred Provider");

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: makeSettings(),
      }),
    );
    expect(await screen.findByText("Settings saved.")).toBeInTheDocument();
  });

  it("keeps automatic revision reconciliation off by default and persists opt-in", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    const reconciliation = screen.getByRole("switch", {
      name: /automatic revision reconciliation/i,
    });
    expect(reconciliation).toHaveAttribute("aria-checked", "false");

    await user.click(reconciliation);
    expect(reconciliation).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          auto_revision_reconciliation: true,
        },
      }),
    );
  });

  it("shows an error state with a working back button when loading fails", async () => {
    invoke.mockRejectedValue(new Error("disk on fire"));
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<SettingsPage onClose={onClose} dark={false} onDarkChange={() => {}} />);

    expect(
      await screen.findByText(/Failed to load settings/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go back" }));
    expect(onClose).toHaveBeenCalled();
  });
});
