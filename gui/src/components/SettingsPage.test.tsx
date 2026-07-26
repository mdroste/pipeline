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
    marker_force_ocr: false,
    marker_disable_images: true,
    marker_lowres_dpi: 96,
    marker_highres_dpi: 192,
    marker_pdftext_workers: 0,
    marker_layout_batch_size: 0,
    marker_recognition_batch_size: 0,
    paddle_page_concurrency: 0,
    paddle_mtmd_batch_tokens: 0,
    paddle_flash_attention: "auto",
    paddle_max_output_tokens: 4096,
    paddle_page_retries: 1,
    paddle_render_dpi: 150,
    pdf_extraction_timeout_secs: 900,
    reuse_pdf_extraction_cache: true,
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

  it("keeps Marker tuning available when another global extractor is selected", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));

    expect(screen.getByLabelText("Marker OCR mode")).toHaveValue("auto");
    await user.selectOptions(screen.getByLabelText("Marker OCR mode"), "forced");
    await user.selectOptions(screen.getByLabelText("Marker layout resolution"), "72");
    await user.selectOptions(screen.getByLabelText("Marker OCR resolution"), "144");
    await user.selectOptions(screen.getByLabelText("Marker PDF text workers"), "8");
    await user.selectOptions(screen.getByLabelText("Marker layout batch"), "12");
    await user.selectOptions(screen.getByLabelText("Marker OCR recognition batch"), "32");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          marker_force_ocr: true,
          marker_lowres_dpi: 72,
          marker_highres_dpi: 144,
          marker_pdftext_workers: 8,
          marker_layout_batch_size: 12,
          marker_recognition_batch_size: 32,
        },
      }),
    );
  });

  it("shows and saves PaddleOCR-VL performance controls", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));
    await user.click(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Q8/,
      }),
    );

    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL concurrent pages"),
      "2",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL vision encoder batch"),
      "2048",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL page resolution"),
      "120",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL Flash Attention"),
      "on",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL maximum page output"),
      "8192",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL page retries"),
      "2",
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          pdf_extractor: "paddleocr-vl",
          paddle_page_concurrency: 2,
          paddle_render_dpi: 120,
          paddle_mtmd_batch_tokens: 2048,
          paddle_flash_attention: "on",
          paddle_max_output_tokens: 8192,
          paddle_page_retries: 2,
        },
      }),
    );
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
