import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPage from "./SettingsPage";
import type { ModelCatalog, Settings } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

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
    paddle_full_layout_detection: true,
    paddle_full_layout_threshold: 0.5,
    paddle_full_layout_nms: true,
    paddle_full_layout_merge_bboxes_mode: "large",
    paddle_full_merge_layout_blocks: true,
    paddle_full_ocr_image_blocks: true,
    paddle_full_format_block_content: true,
    paddle_full_merge_tables: true,
    paddle_full_relevel_titles: true,
    paddle_full_show_formula_numbers: true,
    pdf_extraction_timeout_secs: 1800,
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
    if (cmd === "retired_marker_status") {
      return Promise.resolve({ present: false, bytes: 0 });
    }
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

function catalog(
  provider: string,
  transport: "cli" | "api",
  sourceVersion: string,
): ModelCatalog {
  return {
    provider,
    transport,
    source: "test",
    source_version: sourceVersion,
    fetched_at: "2026-07-27T00:00:00Z",
    stale: false,
    models: [],
    roles: [],
  };
}

describe("SettingsPage", () => {
  beforeEach(() => {
    invoke.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("loads settings and renders the LLM provider section", async () => {
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    expect(await screen.findByText("Preferred Provider")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Claude (Anthropic)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "ChatGPT (OpenAI)" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Preferred Provider" })).toBeVisible();
    expect(screen.getByLabelText("Claude API Key")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "claude model" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("get_settings");
  });

  it("surfaces a shell-plugin failure when opening the Ollama site", async () => {
    mockLoad(makeSettings());
    openUrl.mockRejectedValueOnce(new Error("no browser"));
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await userEvent.setup().click(await screen.findByRole("link", { name: "ollama.com" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open ollama.com: no browser",
    );
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
    expect(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    ).toBeInTheDocument();
  });

  it("explains a legacy Marker selection and saves a supported replacement", async () => {
    const user = userEvent.setup();
    mockLoad({ ...makeSettings(), pdf_extractor: "marker" });
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Marker is unavailable in Pipeline 1.0.1",
    );
    expect(
      screen.queryByRole("radio", { name: /marker-pdf/i }),
    ).not.toBeInTheDocument();
    await user.click(
      screen.getByRole("radio", { name: /pdftotext \(basic\)/i }),
    );
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          pdf_extractor: "pdftotext",
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

  it("shows and saves full-parser structure controls", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));
    await user.click(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL layout confidence threshold"),
      "0.7",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL overlapping layout boxes"),
      "union",
    );
    await user.click(screen.getByRole("switch", { name: "Merge tables across pages" }));
    await user.click(screen.getByRole("switch", { name: "Retain formula numbers" }));
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          pdf_extractor: "paddleocr-vl-full",
          paddle_full_layout_threshold: 0.7,
          paddle_full_layout_merge_bboxes_mode: "union",
          paddle_full_merge_tables: false,
          paddle_full_show_formula_numbers: false,
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

  it("keeps edits made during a save dirty", async () => {
    const user = userEvent.setup();
    let finishSave!: () => void;
    const pendingSave = new Promise<void>((resolve) => {
      finishSave = resolve;
    });
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "save_settings") return pendingSave;
      if (cmd === "list_engines") return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onDirtyChange = vi.fn();
    render(
      <SettingsPage
        onClose={() => {}}
        onDirtyChange={onDirtyChange}
        dark={false}
        onDarkChange={() => {}}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "General" }));
    const reconciliation = screen.getByRole("switch", {
      name: /automatic revision reconciliation/i,
    });
    await user.click(reconciliation);
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await user.click(reconciliation);
    await act(async () => finishSave());

    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));
    expect(screen.queryByText("Settings saved.")).not.toBeInTheDocument();
  });

  it("does not discover a partial unsaved credential and disables its stale catalog", async () => {
    let claudeRequests = 0;
    invoke.mockImplementation((cmd: string, args?: { provider?: string }) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        if (args?.provider === "claude") {
          claudeRequests += 1;
          return Promise.resolve(catalog("claude", "cli", "saved-account"));
        }
        return Promise.resolve(catalog(args?.provider ?? "local", "cli", "current"));
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    expect(await screen.findByText("CLI · saved-account")).toBeVisible();
    expect(claudeRequests).toBe(1);
    invoke.mockClear();

    await user.type(screen.getByLabelText("Claude API Key"), "s");
    const model = screen.getByRole("combobox", { name: "claude model" });
    await waitFor(() => expect(model).toBeDisabled());
    expect(screen.queryByText("CLI · saved-account")).not.toBeInTheDocument();
    expect(
      screen.getByText("Save settings or Refresh to discover models for these values"),
    ).toBeVisible();
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_model_catalog" &&
          (args as { provider?: string } | undefined)?.provider === "claude",
      ),
    ).toHaveLength(0);
  });

  it("forces a cloud catalog refresh only after the changed credential is saved", async () => {
    let claudeRequests = 0;
    invoke.mockImplementation((cmd: string, args?: {
      provider?: string;
      refresh?: boolean;
      settings?: Settings;
    }) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        if (args?.provider === "claude") {
          claudeRequests += 1;
          return Promise.resolve(
            catalog(
              "claude",
              claudeRequests === 1 ? "cli" : "api",
              claudeRequests === 1 ? "saved-account" : "new-account",
            ),
          );
        }
        return Promise.resolve(catalog(args?.provider ?? "local", "cli", "current"));
      }
      if (cmd === "save_settings") return Promise.resolve();
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    expect(await screen.findByText("CLI · saved-account")).toBeVisible();
    await user.type(screen.getByLabelText("Claude API Key"), "sk-complete-key");
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "claude model" })).toBeDisabled(),
    );
    expect(claudeRequests).toBe(1);

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("API · new-account")).toBeVisible();
    expect(screen.getByRole("combobox", { name: "claude model" })).toBeEnabled();
    expect(claudeRequests).toBe(2);
    expect(invoke).toHaveBeenCalledWith("get_model_catalog", expect.objectContaining({
      provider: "claude",
      refresh: true,
      settings: expect.objectContaining({ anthropic_api_key: "sk-complete-key" }),
    }));
  });

  it("ignores an in-flight saved-account response after a draft key is saved", async () => {
    let resolveOld!: (value: ModelCatalog) => void;
    const oldCatalog = new Promise<ModelCatalog>((resolve) => {
      resolveOld = resolve;
    });
    let claudeRequests = 0;
    invoke.mockImplementation((cmd: string, args?: {
      provider?: string;
      refresh?: boolean;
      settings?: Settings;
    }) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        if (args?.provider === "claude") {
          claudeRequests += 1;
          return claudeRequests === 1
            ? oldCatalog
            : Promise.resolve(catalog("claude", "api", "api-new"));
        }
        return Promise.resolve(catalog(args?.provider ?? "local", "cli", "current"));
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "retired_marker_status") return Promise.resolve({ present: false, bytes: 0 });
      if (cmd === "save_settings") return Promise.resolve();
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    const key = await screen.findByPlaceholderText("sk-ant-... (optional, enables direct API)");
    await waitFor(() => expect(claudeRequests).toBe(1));
    await user.type(key, "sk-complete-key");
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "claude model" })).toBeDisabled(),
    );
    expect(claudeRequests).toBe(1);
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("API · api-new")).toBeVisible();
    expect(claudeRequests).toBe(2);
    expect(invoke).toHaveBeenCalledWith("get_model_catalog", expect.objectContaining({
      provider: "claude",
      refresh: true,
      settings: expect.objectContaining({ anthropic_api_key: "sk-complete-key" }),
    }));

    await act(async () => resolveOld(catalog("claude", "cli", "cli-old")));
    expect(screen.getByText("API · api-new")).toBeVisible();
    expect(screen.queryByText("CLI · cli-old")).not.toBeInTheDocument();
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

  it("previews an exact purge and reports the number actually removed", async () => {
    const user = userEvent.setup();
    let usageCalls = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "runs_disk_usage") {
        usageCalls += 1;
        return Promise.resolve(
          usageCalls === 1
            ? { count: 8, bytes: 7_000_000_000 }
            : { count: 5, bytes: 4_500_000_000 },
        );
      }
      if (cmd === "preview_purge_runs") {
        return Promise.resolve({
          delete_count: 3,
          delete_bytes: 2_500_000_000,
          remaining_count: 5,
          remaining_bytes: 4_500_000_000,
          preview_token: "confirmed-plan",
        });
      }
      if (cmd === "purge_runs") return Promise.resolve(3);
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await screen.findByText(/Currently 8 runs, 7\.0 GB/);
    await user.click(screen.getByRole("button", { name: "Purge now" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("preview_purge_runs", {
        keep: 0,
        maxBytes: 5_000_000_000,
      });
      expect(invoke).toHaveBeenCalledWith("purge_runs", {
        keep: 0,
        maxBytes: 5_000_000_000,
        previewToken: "confirmed-plan",
      });
    });
    expect(confirmSpy.mock.calls[0][0]).toContain(
      "permanently delete 3 completed runs (2.5 GB)",
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Removed 3 completed runs.",
    );
    confirmSpy.mockRestore();
  });

  it("does not ask for destructive confirmation when the preview is empty", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "runs_disk_usage") {
        return Promise.resolve({ count: 5, bytes: 4_500_000_000 });
      }
      if (cmd === "preview_purge_runs") {
        return Promise.resolve({
          delete_count: 0,
          delete_bytes: 0,
          remaining_count: 5,
          remaining_bytes: 4_500_000_000,
          preview_token: "empty-plan",
        });
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const confirmSpy = vi.spyOn(window, "confirm");
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await user.click(screen.getByRole("button", { name: "Purge now" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "No completed runs were beyond the configured limits.",
    );
    expect(confirmSpy).not.toHaveBeenCalled();
    expect(invoke.mock.calls.some(([command]) => command === "purge_runs")).toBe(false);
    confirmSpy.mockRestore();
  });

  it("does not purge when the exact preview is not confirmed", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "runs_disk_usage") {
        return Promise.resolve({ count: 8, bytes: 7_000_000_000 });
      }
      if (cmd === "preview_purge_runs") {
        return Promise.resolve({
          delete_count: 3,
          delete_bytes: 2_500_000_000,
          remaining_count: 5,
          remaining_bytes: 4_500_000_000,
          preview_token: "confirmed-plan",
        });
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await user.click(screen.getByRole("button", { name: "Purge now" }));
    await waitFor(() => expect(confirmSpy).toHaveBeenCalledTimes(1));
    expect(invoke.mock.calls.some(([command]) => command === "purge_runs")).toBe(false);
    confirmSpy.mockRestore();
  });

  it("stops safely and shows an error when purge preview fails", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "runs_disk_usage") {
        return Promise.resolve({ count: 8, bytes: 7_000_000_000 });
      }
      if (cmd === "preview_purge_runs") {
        return Promise.reject(new Error("history index unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(<SettingsPage onClose={() => {}} dark={false} onDarkChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await user.click(screen.getByRole("button", { name: "Purge now" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The purge preview could not be loaded; no runs were deleted: history index unavailable",
    );
    expect(invoke.mock.calls.some(([command]) => command === "purge_runs")).toBe(false);
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
