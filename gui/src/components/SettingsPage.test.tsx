import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPage from "./SettingsPage";
import type { EngineStatus, ModelCatalog, Settings } from "../lib/types";

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
    max_workers: 16,
    active_profile: "deep-review",
    claude_model: "",
    claude_effort: "",
    codex_model: "",
    codex_effort: "",
    antigravity_effort: "",
    pdf_extractor: "llm",
    paddle_page_concurrency: 0,
    paddle_mtmd_batch_tokens: 0,
    paddle_flash_attention: "auto",
    paddle_max_output_tokens: 4096,
    paddle_page_retries: 1,
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
    claude_access_mode: "subscription",
    codex_access_mode: "subscription",
    antigravity_access_mode: "subscription",
    anthropic_api_key: "",
    openai_api_key: "",
    google_api_key: "",
    local_base_url: "http://localhost:11434/v1",
    local_model: "",
    local_api_key: "",
  };
}

function paddleEngine(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    id: "paddleocr-vl-parser",
    label: "PaddleOCR-VL 1.6 Full Parser",
    description: "Layout-aware local extraction.",
    installed: false,
    version: "",
    entry_path: "",
    system_path: "",
    est_download_mb: 2900,
    est_disk_mb: 3800,
    managed_stack_mb: 0,
    installing: false,
    install_progress: null,
    ...overrides,
  };
}

function mockLoad(
  settings: Settings,
  warnings: string[] = [],
  engines: EngineStatus[] = [],
) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings") return Promise.resolve({ settings, warnings });
    if (cmd === "save_settings") return Promise.resolve();
    if (cmd === "list_engines") return Promise.resolve(engines);
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

  it("keeps role defaults on Models and removes provider configuration boxes", async () => {
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    expect(await screen.findByText("Preferred Provider")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Claude (Anthropic)" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "ChatGPT (OpenAI)" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Preferred Provider" })).toBeVisible();
    expect(screen.queryByText("Provider configuration")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Claude API Key")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Maximum Concurrent Agents")).not.toBeInTheDocument();
    expect(screen.getByText("or more", { selector: "strong" }).closest("p")).toHaveTextContent(
      "Select one or more default model to use for parallel workflow steps.",
    );
    expect(screen.getByText("one", { selector: "strong" }).closest("p")).toHaveTextContent(
      "Select one default model to use for sequential workflow steps.",
    );
    expect(screen.getByText(
      "Select one default model to use for processing inputs and classifying adaptive workflow steps.",
    )).toBeVisible();
    expect(screen.getByRole("button", { name: "API Keys" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Workflow" })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("get_settings");
  });

  it("keeps API credentials on their own page with explicit connection modes", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "API Keys" }));
    expect(screen.getByRole("heading", { name: "API Keys" })).toBeVisible();
    expect(screen.getByLabelText("Claude API Key")).toBeVisible();
    expect(screen.getByLabelText("OpenAI API Key")).toBeVisible();
    expect(screen.getByLabelText("Gemini API Key")).toBeVisible();
    expect(screen.getByLabelText("Local API Key")).toBeVisible();
    expect(screen.getAllByRole("radio", { name: "Subscription" })[0]).toBeChecked();

    const claudeModes = screen.getByRole("radiogroup", { name: "Claude connection mode" });
    const api = claudeModes.querySelector<HTMLInputElement>('input[value="api"]');
    expect(api).not.toBeNull();
    await user.click(api!);
    expect(api).toBeChecked();
    expect(screen.getByText(/Enter an Anthropic API key/)).toBeVisible();
  });

  it("moves execution controls to Workflow", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await screen.findByText("Preferred Provider");
    expect(screen.queryByLabelText("Maximum Concurrent Agents")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Workflow" }));

    const maxConcurrentAgents = screen.getByLabelText("Maximum Concurrent Agents");
    expect(maxConcurrentAgents).toHaveValue("16");
    expect(maxConcurrentAgents).toHaveAttribute("max", "20");
    expect(screen.getByLabelText("Step Timeout")).toHaveValue("1200");
    expect(screen.getByLabelText("Step Retries")).toHaveValue("1");
  });

  it("opens directly to a requested settings section", async () => {
    mockLoad(makeSettings());
    const view = render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="llm"
      />,
    );
    expect(await screen.findByText("Preferred Provider")).toBeVisible();

    view.rerender(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="extraction"
      />,
    );

    expect(await screen.findByRole("heading", { name: "PDF Extraction" })).toBeVisible();
    expect(screen.getByRole("radio", { name: /^LLM\b/ })).toBeVisible();
  });

  it("focuses and scrolls to a requested settings target", async () => {
    const originalScrollIntoView = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "scrollIntoView",
    );
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });
    try {
      mockLoad(makeSettings());
      render(
        <SettingsPage
          onClose={() => {}}
          theme="light"
          onThemeChange={() => {}}
          initialSection="extraction"
          targetId="paddleocr-local-engine"
          navigationKey={1}
        />,
      );

      const target = await waitFor(() => {
        const element = document.getElementById("paddleocr-local-engine");
        expect(element).not.toBeNull();
        return element as HTMLElement;
      });
      await waitFor(() => {
        expect(scrollIntoView).toHaveBeenCalledWith({ block: "start" });
        expect(target).toHaveFocus();
      });
    } finally {
      if (originalScrollIntoView) {
        Object.defineProperty(
          HTMLElement.prototype,
          "scrollIntoView",
          originalScrollIntoView,
        );
      } else {
        delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
      }
    }
  });

  it("lists discovered models without stable role options", async () => {
    invoke.mockImplementation((cmd: string, args?: { provider?: string }) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        const result = catalog(args?.provider ?? "local", "cli", "live");
        if (args?.provider === "claude") {
          result.roles = [{
            id: "balanced",
            label: "Balanced",
            description: "Stable balanced role",
            model: "claude-live",
          }];
          result.models = [{
            id: "claude-live",
            display_name: "Claude Live",
            description: "Discovered model",
            is_default: false,
            supported_efforts: [],
            capabilities: [],
            deprecated: false,
          }];
        }
        return Promise.resolve(result);
      }
      return Promise.resolve();
    });

    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    expect(await screen.findByRole("option", {
      name: "Claude Live · Parallel steps",
    })).toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "Exact model" })).toHaveLength(3);
    expect(screen.queryByRole("group", { name: "Stable roles" })).not.toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /Balanced/ })).not.toBeInTheDocument();
  });

  it("surfaces a shell-plugin failure when opening the Ollama site", async () => {
    mockLoad(makeSettings());
    openUrl.mockRejectedValueOnce(new Error("no browser"));
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", {
      name: "More information about Local server",
    }));
    await user.click(screen.getByRole("link", { name: "ollama.com" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open ollama.com: no browser",
    );
  });

  it("keeps secondary explanations in accessible info popovers", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await screen.findByText("Preferred Provider");
    expect(
      screen.queryByText("Provider used when workflow does not specify explicit agent(s)."),
    ).not.toBeInTheDocument();

    const info = screen.getByRole("button", {
      name: "More information about Preferred Provider",
    });
    await user.click(info);

    expect(screen.getByRole("dialog", { name: "Preferred Provider help" })).toHaveTextContent(
      "Provider used when workflow does not specify explicit agent(s).",
    );
    expect(info).toHaveAttribute("aria-expanded", "true");

    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Preferred Provider help" })).not.toBeInTheDocument();
    expect(info).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "General" }));
    const logging = screen.getByRole("switch", { name: "Verbose console logging" });
    await user.click(screen.getByRole("button", {
      name: "More information about Verbose console logging",
    }));
    expect(logging).toHaveAttribute("aria-checked", "false");
    await user.click(screen.getByText("Verbose console logging"));
    expect(logging).toHaveAttribute("aria-checked", "true");
  });

  it("offers system, light, and dark appearance choices", async () => {
    const user = userEvent.setup();
    const onThemeChange = vi.fn();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="system"
        onThemeChange={onThemeChange}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "General" }));
    const appearance = screen.getByRole("combobox", { name: "Appearance" });
    expect(appearance).toHaveValue("system");
    expect(screen.getByRole("option", { name: "System" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Dark" })).toBeInTheDocument();

    await user.selectOptions(appearance, "dark");
    expect(onThemeChange).toHaveBeenCalledWith("dark");
  });

  it("offers only the PaddleOCR-VL Full Parser as a local PDF extractor", async () => {
    mockLoad(makeSettings(), [], [paddleEngine()]);
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "PDF Extraction" }));
    expect(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("radio", { name: /PaddleOCR-VL 1\.6 Q8/ }),
    ).not.toBeInTheDocument();
    const engineHeading = screen.getByRole("heading", { name: "Local Engines" });
    expect(engineHeading).toBeVisible();
    expect(screen.getByText("not installed")).toBeVisible();
    expect(screen.queryByText("PaddleOCR-VL recognition server")).not.toBeInTheDocument();
    expect(screen.queryByText("Full parser structure")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("PaddleOCR-VL concurrent pages")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("switch", { name: "Layout detection and reading order" }),
    ).not.toBeInTheDocument();
  });

  it("ranks manual PDF extraction methods from best to basic quality", async () => {
    mockLoad(makeSettings(), [], [paddleEngine()]);
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "PDF Extraction" }));

    const paddle = screen.getByRole("radio", {
      name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
    });
    const llm = screen.getByRole("radio", { name: /^LLM\b/ });
    const pdftotext = screen.getByRole("radio", { name: /^pdftotext\b/ });

    expect(screen.getByText("Best to basic ↓")).toBeVisible();
    expect(screen.getByText("Best quality")).toBeVisible();
    expect(screen.getByText("High quality")).toBeVisible();
    expect(screen.getByText("Basic quality")).toBeVisible();
    expect(screen.getByText(/Slightly less faithful than PaddleOCR-VL/)).toBeVisible();
    expect(
      paddle.compareDocumentPosition(llm) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      llm.compareDocumentPosition(pdftotext) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("reveals PaddleOCR-VL settings when the local engine is installed", async () => {
    mockLoad(makeSettings(), [], [paddleEngine({ installed: true })]);
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "PDF Extraction" }));

    const engineHeading = screen.getByRole("heading", { name: "Local Engines" });
    const recognitionSettings = await screen.findByText("PaddleOCR-VL recognition server");
    expect(
      engineHeading.compareDocumentPosition(recognitionSettings) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByText("Full parser structure")).toBeVisible();
  });

  it("shows the automatic default using an installed Full Parser", async () => {
    const user = userEvent.setup();
    mockLoad(
      { ...makeSettings(), pdf_extractor: "auto" },
      [],
      [paddleEngine({ installed: true })],
    );
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));

    expect(screen.getByRole("radio", { name: /^Automatic/ })).toBeChecked();
    expect(
      await screen.findByText(/Full Parser is installed, so Pipeline will use it/),
    ).toBeVisible();

    await user.click(screen.getByRole("radio", { name: /^LLM/ }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), pdf_extractor: "llm" },
      }),
    );
  });

  it("shows and saves PaddleOCR-VL performance controls", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings(), [], [paddleEngine({ installed: true })]);
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "PDF Extraction" }));
    await user.click(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
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

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          pdf_extractor: "paddleocr-vl-full",
          paddle_page_concurrency: 2,
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
    mockLoad(makeSettings(), [], [paddleEngine({ installed: true })]);
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    expect(
      await screen.findByText(/settings\.json\.corrupt/),
    ).toBeInTheDocument();
  });

  it("automatically saves changes and shows the saved indicator", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await screen.findByText("Preferred Provider");

    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
    expect(screen.getByText("Changes save automatically.")).toBeVisible();
    await user.selectOptions(screen.getByLabelText("Preferred Provider"), "codex");

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), preferred_provider: "codex" },
      }),
    );
    expect(await screen.findByText("All changes saved.")).toBeInTheDocument();
  });

  it("shows an inline autosave error and retries the current settings", async () => {
    const user = userEvent.setup();
    let saveAttempts = 0;
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    invoke.mockImplementation((cmd: string, args?: { provider?: string }) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        return Promise.resolve(catalog(args?.provider ?? "local", "cli", "current"));
      }
      if (cmd === "save_settings") {
        saveAttempts += 1;
        return saveAttempts === 1
          ? Promise.reject(new Error("settings file is locked"))
          : Promise.resolve();
      }
      return Promise.resolve();
    });

    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);
    await user.selectOptions(
      await screen.findByLabelText("Preferred Provider"),
      "codex",
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save settings: settings file is locked",
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(saveAttempts).toBe(2));
    expect(await screen.findByText("All changes saved.")).toBeInTheDocument();
    consoleError.mockRestore();
  });

  it("serializes an edit made during an autosave and persists the latest value", async () => {
    const user = userEvent.setup();
    let finishFirstSave!: () => void;
    const firstSave = new Promise<void>((resolve) => {
      finishFirstSave = resolve;
    });
    let saveCalls = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "save_settings") {
        saveCalls += 1;
        return saveCalls === 1 ? firstSave : Promise.resolve();
      }
      if (cmd === "list_engines") return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    const onDirtyChange = vi.fn();
    render(
      <SettingsPage
        onClose={() => {}}
        onDirtyChange={onDirtyChange}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "General" }));
    const reconciliation = screen.getByRole("switch", {
      name: /automatic revision reconciliation/i,
    });
    await user.click(reconciliation);
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: { ...makeSettings(), auto_revision_reconciliation: true },
    }));
    await user.click(reconciliation);
    await act(async () => finishFirstSave());

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: makeSettings(),
    }));
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(false));
    expect(await screen.findByText("All changes saved.")).toBeInTheDocument();
  });

  it("does not discover a partial credential before debounced autosave", async () => {
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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await waitFor(() => expect(claudeRequests).toBe(1));
    invoke.mockClear();

    await user.click(screen.getByRole("button", { name: "API Keys" }));
    await user.type(screen.getByLabelText("Claude API Key"), "s");
    expect(invoke.mock.calls.some(([command]) => command === "save_settings")).toBe(false);
    expect(
      invoke.mock.calls.filter(
        ([command, args]) =>
          command === "get_model_catalog" &&
          (args as { provider?: string } | undefined)?.provider === "claude",
      ),
    ).toHaveLength(0);
  });

  it("refreshes a cloud catalog after an API mode and credential change is saved", async () => {
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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await waitFor(() => expect(claudeRequests).toBe(1));
    await user.click(screen.getByRole("button", { name: "API Keys" }));
    const claudeModes = screen.getByRole("radiogroup", { name: "Claude connection mode" });
    await user.click(claudeModes.querySelector<HTMLInputElement>('input[value="api"]')!);
    await user.type(screen.getByLabelText("Claude API Key"), "sk-complete-key");
    expect(claudeRequests).toBe(1);

    await waitFor(() => expect(claudeRequests).toBe(2));
    expect(invoke).toHaveBeenCalledWith("get_model_catalog", expect.objectContaining({
      provider: "claude",
      refresh: true,
      settings: expect.objectContaining({
        anthropic_api_key: "sk-complete-key",
        claude_access_mode: "api",
      }),
    }));
  });

  it("keeps automatic revision reconciliation off by default and persists opt-in", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    const reconciliation = screen.getByRole("switch", {
      name: /automatic revision reconciliation/i,
    });
    expect(reconciliation).toHaveAttribute("aria-checked", "false");

    await user.click(reconciliation);
    expect(reconciliation).toHaveAttribute("aria-checked", "true");

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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await screen.findByText("8 reports · 7.0 GB");
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
      "permanently delete 3 completed reports (2.5 GB)",
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Removed 3 completed reports.",
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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await user.click(screen.getByRole("button", { name: "Purge now" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "No completed reports were beyond the configured limits.",
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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

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
    render(<SettingsPage onClose={() => {}} theme="light" onThemeChange={() => {}} />);

    await user.click(await screen.findByRole("button", { name: "General" }));
    await user.click(screen.getByRole("button", { name: "Purge now" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The purge preview could not be loaded; no reports were deleted: history index unavailable",
    );
    expect(invoke.mock.calls.some(([command]) => command === "purge_runs")).toBe(false);
  });

  it("shows an error state with a working back button when loading fails", async () => {
    invoke.mockRejectedValue(new Error("disk on fire"));
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(<SettingsPage onClose={onClose} theme="light" onThemeChange={() => {}} />);

    expect(
      await screen.findByText(/Failed to load settings/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go back" }));
    expect(onClose).toHaveBeenCalled();
  });
});
