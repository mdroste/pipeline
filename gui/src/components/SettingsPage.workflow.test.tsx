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

    await screen.findByText("Preferred Provider");
    await user.click(screen.getByText("Combine results"));
    const mergeProviders = screen.getByRole("group", {
      name: "Merge providers",
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

    await screen.findByText("Preferred Provider");
    expect(screen.getByLabelText("Maximum Concurrent Agents")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Reviews" }));

    const maxConcurrentAgents = screen.getByLabelText(
      "Maximum Concurrent Agents",
    );
    expect(maxConcurrentAgents).toHaveValue("16");
    expect(maxConcurrentAgents).toHaveAttribute("max", "20");
    expect(screen.getByLabelText("Step Timeout")).toHaveValue("1200");
    expect(screen.getByLabelText("Step Retries")).toHaveValue("1");
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
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );
    expect(
      screen.getByRole("radio", {
        name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("radio", { name: /PaddleOCR-VL 1\.6 Q8/ }),
    ).not.toBeInTheDocument();
    const engineHeading = screen.getByRole("heading", {
      name: "Local Engines",
    });
    expect(engineHeading).toBeVisible();
    expect(screen.getByText("not installed")).toBeVisible();
    expect(
      screen.queryByText("PaddleOCR-VL recognition server"),
    ).not.toBeVisible();
    expect(screen.queryByText("Full parser structure")).not.toBeVisible();
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
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );

    const paddle = screen.getByRole("radio", {
      name: /Local engine: PaddleOCR-VL 1\.6 Full Parser/,
    });
    const llm = screen.getByRole("radio", { name: /^LLM\b/ });
    const pdftotext = screen.getByRole("radio", { name: /^pdftotext\b/ });

    expect(screen.getByText("Best to basic ↓")).toBeVisible();
    expect(screen.getByText("Best quality")).toBeVisible();
    expect(screen.getByText("High quality")).toBeVisible();
    expect(screen.getByText("Basic quality")).toBeVisible();
    expect(
      screen.getByText(/This may lose more detail than PaddleOCR-VL/),
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
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );
    expect(await screen.findByText("Full parser structure")).not.toBeVisible();
    await userEvent.click(await screen.findByText("Advanced parser settings"));

    const engineHeading = screen.getByRole("heading", {
      name: "Local Engines",
    });
    const recognitionSettings = await screen.findByText(
      "PaddleOCR-VL recognition server",
    );
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
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );

    expect(screen.getByRole("radio", { name: /^Automatic/ })).toBeChecked();
    expect(
      await screen.findByText(
        /Full Parser is installed, so Pipeline will use it/,
      ),
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
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );
    await userEvent.click(await screen.findByText("Advanced parser settings"));
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
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.click(
      await screen.findByRole("button", { name: "PDF Extraction" }),
    );
    await userEvent.click(await screen.findByText("Advanced parser settings"));
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
    await user.click(
      screen.getByRole("switch", { name: "Merge tables across pages" }),
    );
    await user.click(
      screen.getByRole("switch", { name: "Retain formula numbers" }),
    );

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

  it("keeps automatic revision reconciliation off by default and persists opt-in", async () => {
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
});
