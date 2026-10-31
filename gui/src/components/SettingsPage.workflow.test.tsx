import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPageComponent from "./SettingsPage";
import type { ComponentProps } from "react";
// Existing Review behavior tests enter Reviews explicitly after the navigation redesign.
const SettingsPage = (props: ComponentProps<typeof SettingsPageComponent>) => (
  <SettingsPageComponent initialSection="workflow" {...props} />
);
import { makeSettings, paddleEngine } from "../test/settingsFixtures";
import type { EngineStatus, Settings } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));
vi.mock("./DialogService", () => ({ confirmDialog, notify: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

import { mockSettingsLoad } from "../test/settingsPageFixtures";

const mockLoad = (
  settings: Settings,
  warnings: string[] = [],
  engines: EngineStatus[] = [],
) => mockSettingsLoad(invoke, settings, warnings, engines);

describe("SettingsPage workflow", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirmDialog.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("saves a dedicated Merge provider default", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await screen.findByText("Model defaults");
    await user.click(screen.getAllByText("Combine results")[0]);
    const mergeProviders = screen.getByRole("group", {
      name: "Combine results providers",
    });
    await user.click(
      within(mergeProviders).getByRole("button", { name: "ChatGPT" }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          default_merge_agent: "codex",
          default_merge_model_overrides: {},
          default_merge_effort_overrides: {},
        },
      }),
    );
  });

  it("lets a stage go back to following the default", async () => {
    const user = userEvent.setup();
    const settings = {
      ...makeSettings(),
      default_sequential_agent: "codex",
      default_sequential_model_overrides: {
        "codex:cli": { mode: "pinned" as const, model: "sol" },
      },
    };
    mockLoad(settings);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await screen.findByText("Model defaults");
    await user.click(screen.getAllByText("Sequential steps")[0]);
    await user.click(
      within(
        screen.getByRole("group", { name: "Sequential steps providers" }),
      ).getByRole("button", { name: "Default model" }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...settings,
          default_sequential_agent: "",
          default_sequential_model_overrides: {},
          default_sequential_effort_overrides: {},
        },
      }),
    );
  });

  it("offers a connected provider as the usage-limit backup", async () => {
    const user = userEvent.setup();
    mockLoad({ ...makeSettings(), openai_api_key: "", google_api_key: "key" });
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        dependencies={{
          ready: true,
          deps: [
            {
              name: "ChatGPT (reviews)",
              found: true,
              version: "",
              path: "",
              required: false,
              hint: "",
              authenticated: false,
            },
          ],
        }}
      />,
    );

    await user.click(
      await screen.findByLabelText("Enable usage-limit fallback"),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "save_settings",
        expect.objectContaining({
          settings: expect.objectContaining({
            usage_limit_fallback_agent: "antigravity",
          }),
        }),
      ),
    );
  });

  it("explains that a backup needs a second connected provider", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        dependencies={{
          ready: true,
          deps: [
            {
              name: "ChatGPT (reviews)",
              found: true,
              version: "",
              path: "",
              required: false,
              hint: "",
              authenticated: false,
            },
            {
              name: "Local LLM server",
              found: false,
              version: "",
              path: "",
              required: false,
              hint: "",
            },
          ],
        }}
      />,
    );
    expect(
      await screen.findByLabelText("Enable usage-limit fallback"),
    ).toBeDisabled();
    expect(
      screen.getByText(/Connect a second provider to use a backup/),
    ).toBeVisible();
  });

  it("keeps execution controls alongside model defaults", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await screen.findByText("Model defaults");
    expect(screen.getByLabelText("Model calls at once")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Reviews" }));

    const maxConcurrentAgents = screen.getByLabelText("Model calls at once");
    expect(maxConcurrentAgents).toHaveValue("16");
    expect(maxConcurrentAgents).toHaveAttribute("max", "20");
    expect(screen.getByLabelText("Step time limit")).toHaveValue("1200");
    expect(screen.getByLabelText("Step retries")).toHaveValue("1");
  });

  it("offers only the PaddleOCR-VL Full Parser as a local PDF extractor", async () => {
    mockLoad(makeSettings(), [], [paddleEngine()]);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );
    expect(
      screen.getByRole("radio", {
        name: /Local parser: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("radio", { name: /PaddleOCR-VL 1\.6 Q8/ }),
    ).not.toBeInTheDocument();
    const engineHeading = screen.getByRole("heading", {
      name: "Local parser",
    });
    expect(engineHeading).toBeVisible();
    expect(screen.getByText("not installed")).toBeVisible();
    expect(screen.queryByText("Speed and memory")).not.toBeVisible();
    expect(screen.queryByText("Document structure")).not.toBeVisible();
    expect(
      screen.queryByLabelText("PaddleOCR-VL concurrent pages"),
    ).not.toBeVisible();
    expect(
      screen.queryByRole("switch", {
        name: "Layout detection and reading order",
      }),
    ).not.toBeVisible();
  });

  it("ranks manual PDF extraction methods from best to basic quality", async () => {
    mockLoad(makeSettings(), [], [paddleEngine()]);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );

    const paddle = screen.getByRole("radio", {
      name: /Local parser: PaddleOCR-VL 1\.6 Full Parser/,
    });
    const llm = screen.getByRole("radio", { name: /^Selected model\b/ });
    const pdftotext = screen.getByRole("radio", { name: /^pdftotext\b/ });

    expect(screen.getByText("Best to basic ↓")).toBeVisible();
    expect(screen.getByText("Best quality")).toBeVisible();
    expect(screen.getByText("High quality")).toBeVisible();
    expect(screen.getByText("Basic quality")).toBeVisible();
    expect(
      screen.getByText(/It can lose more detail than PaddleOCR-VL/),
    ).toBeVisible();
    expect(
      paddle.compareDocumentPosition(llm) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      llm.compareDocumentPosition(pdftotext) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("reveals PaddleOCR-VL settings when the local engine is installed", async () => {
    mockLoad(makeSettings(), [], [paddleEngine({ installed: true })]);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );
    expect(await screen.findByText("Document structure")).not.toBeVisible();
    await userEvent.click(await screen.findByText("Advanced parser settings"));

    const engineHeading = screen.getByRole("heading", {
      name: "Local parser",
    });
    const recognitionSettings = await screen.findByText("Speed and memory");
    expect(
      engineHeading.compareDocumentPosition(recognitionSettings) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByText("Document structure")).toBeVisible();
  });

  it("shows the automatic default using an installed Full Parser", async () => {
    const user = userEvent.setup();
    mockLoad(
      { ...makeSettings(), pdf_extractor: "auto" },
      [],
      [paddleEngine({ installed: true })],
    );
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );

    expect(screen.getByRole("radio", { name: /^Automatic/ })).toBeChecked();
    expect(
      await screen.findByText(
        /Uses PaddleOCR-VL Full Parser if it’s installed/,
      ),
    ).toBeVisible();

    await user.click(screen.getByRole("radio", { name: /^Selected model/ }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), pdf_extractor: "llm" },
      }),
    );
  });

  it("shows and saves PaddleOCR-VL performance controls", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings(), [], [paddleEngine({ installed: true })]);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );
    await userEvent.click(await screen.findByText("Advanced parser settings"));
    await user.click(
      screen.getByRole("radio", {
        name: /Local parser: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    );

    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL concurrent pages"),
      "2",
    );
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL image batch size"),
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
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF extraction" }),
    );
    await userEvent.click(await screen.findByText("Advanced parser settings"));
    await user.click(
      screen.getByRole("radio", {
        name: /Local parser: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    );
    const layoutThreshold = screen.getByLabelText(
      "PaddleOCR-VL layout confidence threshold",
    );
    expect(layoutThreshold).toHaveDisplayValue("0.30 (recommended)");
    await user.selectOptions(layoutThreshold, "0.7");
    await user.selectOptions(
      screen.getByLabelText("PaddleOCR-VL overlapping layout boxes"),
      "union",
    );
    await user.click(
      screen.getByRole("switch", { name: "Merge tables across pages" }),
    );
    await user.click(
      screen.getByRole("switch", { name: "Keep equation numbers" }),
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          pdf_extractor: "paddleocr-vl-full",
          paddle_full_layout_threshold: 0.7,
          paddle_full_layout_threshold_version: 1,
          paddle_full_layout_merge_bboxes_mode: "union",
          paddle_full_merge_tables: false,
          paddle_full_show_formula_numbers: false,
        },
      }),
    );
  });

  it("keeps the previous-report comparison off by default and persists opt-in", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(await screen.findByRole("button", { name: "Reviews" }));
    const reconciliation = screen.getByRole("switch", {
      name: /compare with the previous report/i,
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
});
