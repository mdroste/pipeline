import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPageComponent from "./SettingsPage";
import type { ComponentProps } from "react";
// Existing Review behavior tests enter Reviews explicitly after the navigation redesign.
const SettingsPage = (props: ComponentProps<typeof SettingsPageComponent>) => (
  <SettingsPageComponent initialSection="workflow" {...props} />
);
import { makeSettings, paddleEngine, catalog } from "../test/settingsFixtures";
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

function mockLoad(
  settings: Settings,
  warnings: string[] = [],
  engines: EngineStatus[] = [],
) {
  invoke.mockImplementation((cmd: string) => {
    if (cmd === "get_settings") return Promise.resolve({ settings, warnings });
    if (cmd === "get_storage_settings")
      return Promise.resolve({
        activeDirectory: "/home/researcher/.pipeline",
        configuredDirectory: "/home/researcher/.pipeline",
        defaultDirectory: "/home/researcher/.pipeline",
        restartRequired: false,
        error: null,
      });
    if (cmd === "workbench_codex_connect") return Promise.resolve({});
    if (cmd === "workbench_codex_account_state")
      return Promise.resolve({ status: "signedOut" });
    if (cmd === "save_settings") return Promise.resolve();
    if (cmd === "list_engines") return Promise.resolve(engines);
    if (cmd === "workflow_codex_status" || cmd === "chatgpt_account_status")
      return Promise.resolve({
        account: { status: "signedOut", email: null, planType: null },
        version: "0.153.4",
        epoch: 1,
        loginInProgress: false,
        unresolvedAttempts: [],
      });
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

describe("SettingsPage", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirmDialog.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("defaults to managed sign-in and keeps legacy activation in advanced settings", async () => {
    mockLoad(makeSettings());
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="providers"
      />,
    );
    expect(
      await screen.findByRole("button", { name: "Sign in to ChatGPT" }),
    ).toBeVisible();
    const selector = screen.getByLabelText("Reviews Codex backend");
    expect(selector).toHaveValue("app_server");
    expect(selector).not.toBeVisible();
    expect(selector.closest("details")).not.toHaveAttribute("open");
    expect(screen.queryByText(/Run.*codex login/)).not.toBeInTheDocument();

    await user.click(screen.getByText("Advanced connection settings"));
    expect(selector).toBeVisible();
    await user.selectOptions(selector, "legacy_cli");
    expect(
      screen.getByRole("group", { name: "ChatGPT account" }),
    ).toBeInTheDocument();
    expect(screen.getByText("codex login")).toBeVisible();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({
          codex_backend: "legacy_cli",
          codex_backend_preference_version: 1,
        }),
      }),
    );

    await user.selectOptions(selector, "app_server");
    expect(
      await screen.findByRole("group", { name: "ChatGPT account" }),
    ).toBeVisible();
    expect(screen.queryByText("codex login")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({
          codex_backend: "app_server",
          codex_backend_preference_version: 1,
        }),
      }),
    );
  });

  it("preserves an explicit advanced legacy choice without starting App Server", async () => {
    mockLoad({
      ...makeSettings(),
      codex_backend: "legacy_cli",
      codex_backend_preference_version: 1,
    });
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="providers"
      />,
    );
    expect(
      (await screen.findAllByText("Subscription · check sign-in"))[0],
    ).toBeVisible();
    expect(screen.getByLabelText("Reviews Codex backend")).toHaveValue(
      "legacy_cli",
    );
    expect(invoke).not.toHaveBeenCalledWith("workflow_codex_status");
  });

  it("consolidates workflow settings while keeping connections on Providers", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    expect(await screen.findByText("Preferred Provider")).toBeInTheDocument();
    for (const label of [
      "Input assessment",
      "Parallel review",
      "Combine results",
      "Sequential steps",
    ])
      await userEvent.click(screen.getAllByText(label)[0]);
    expect(
      screen.getByRole("option", { name: "Claude (Anthropic)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", { name: "ChatGPT (OpenAI)" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: "Preferred Provider" }),
    ).toBeVisible();
    expect(
      screen.queryByText("Provider configuration"),
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Claude API Key")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Maximum Concurrent Agents")).toBeVisible();
    expect(
      screen.getByText("or more", { selector: "strong" }).closest("p"),
    ).toHaveTextContent(
      "Select one or more default model to use for parallel workflow steps.",
    );
    expect(
      screen.getByText("one", { selector: "strong" }).closest("p"),
    ).toHaveTextContent(
      "Select one default model to use for sequential workflow steps.",
    );
    expect(
      screen.getByText(
        "Select one default model to combine outputs when a Parallel step runs with multiple providers.",
      ),
    ).toBeVisible();
    expect(
      screen.getByText(
        "Select one default model to use for processing inputs and classifying adaptive workflow steps.",
      ),
    ).toBeVisible();
    const orientationDefaults = screen.getByRole("group", {
      name: "Orientation map providers",
    });
    const parallelDefaults = screen.getByRole("group", {
      name: "Parallel steps providers",
    });
    expect(
      orientationDefaults.compareDocumentPosition(parallelDefaults) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const navigation = screen.getByRole("navigation", {
      name: "Settings categories",
    });
    expect(within(navigation).getAllByRole("button")).toHaveLength(7);
    expect(
      within(navigation).getByRole("button", { name: "Connections" }),
    ).toBeVisible();
    expect(
      within(navigation).getByRole("button", { name: "Reviews" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.queryByLabelText("Maximum saved reports"),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /^LLM/ })).toBeVisible();
    expect(invoke).toHaveBeenCalledWith("get_settings");
  });

  it("refreshes dependency readiness when the Workflow ChatGPT account changes", async () => {
    const settings = {
      ...makeSettings(),
      codex_backend: "app_server" as const,
    };
    let signedIn = false;
    mockLoad(settings);
    const load = invoke.getMockImplementation()!;
    invoke.mockImplementation((cmd: string, args: unknown) => {
      if (cmd === "workflow_codex_status" || cmd === "chatgpt_account_status")
        return Promise.resolve({
          account: {
            status: signedIn ? "chatgpt" : "signedOut",
            email: null,
            planType: null,
          },
          version: "0.153.4",
          epoch: 1,
          loginInProgress: false,
          unresolvedAttempts: [],
        });
      if (cmd === "get_model_catalog")
        return Promise.resolve(catalog("codex", "cli", "0.153.4"));
      return load(cmd, args);
    });
    const onSystemChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="providers"
        onSystemChange={onSystemChange}
      />,
    );
    await waitFor(() => expect(onSystemChange).toHaveBeenCalledTimes(1));
    signedIn = true;
    const connection = screen.getByRole("group", {
      name: "ChatGPT account",
    });
    await user.click(
      within(connection).getByRole("button", { name: "Refresh" }),
    );
    await waitFor(() => expect(onSystemChange).toHaveBeenCalledTimes(2));
    expect(screen.getByText("Signed in")).toBeVisible();
    expect(
      invoke.mock.calls.some(
        ([cmd]) => cmd === "workbench_save_title_preferences",
      ),
    ).toBe(false);
  });

  it("shows one ChatGPT sign-in for Conversations and Reviews", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="providers"
      />,
    );
    await screen.findByRole("heading", { name: "Connections" });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("chatgpt_account_status"),
    );
    expect(
      screen.getAllByRole("button", { name: "Sign in to ChatGPT" }),
    ).toHaveLength(1);
    expect(
      screen.queryByRole("button", { name: "Sign in with ChatGPT" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Sign in to ChatGPT" }),
    ).toBeVisible();
    expect(screen.queryByLabelText("Title model")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("save_settings", expect.anything());
  });

  it("opens the Workspace sign-in disclosure from an existing Workspace shortcut", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
        initialSection="workspace"
      />,
    );
    expect(
      await screen.findByRole("heading", { name: "Connections" }),
    ).toBeVisible();
    await waitFor(() =>
      expect(document.getElementById("workspace-provider")).toBeVisible(),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("chatgpt_account_status"),
    );
    expect(screen.getByRole("button", { name: "Connections" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("preserves pending edits while moving between providers and workflow defaults", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.selectOptions(
      await screen.findByLabelText("Preferred Provider"),
      "codex",
    );
    await user.click(screen.getByRole("button", { name: "Connections" }));
    await user.type(screen.getByLabelText("Local API Key"), "test-token");
    await user.click(
      screen.getByRole("button", { name: "Save Local API Key" }),
    );
    await user.click(screen.getByRole("button", { name: "Reviews" }));
    expect(screen.getByLabelText("Preferred Provider")).toHaveValue("codex");
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: {
          ...makeSettings(),
          preferred_provider: "codex",
          local_api_key: "test-token",
        },
      }),
    );
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

  it("groups provider credentials, access modes, and local endpoints together", async () => {
    const user = userEvent.setup();
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Connections" }),
    );
    expect(screen.getByRole("heading", { name: "Connections" })).toBeVisible();
    expect(screen.queryByLabelText("Claude API Key")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("OpenAI API Key")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Gemini API Key")).toBeVisible();
    expect(screen.getByLabelText("Local API Key")).toBeVisible();
    expect(
      screen.getAllByRole("radio", { name: "Subscription" })[0],
    ).toBeChecked();

    const claudeModes = screen.getByRole("radiogroup", {
      name: "Claude connection mode",
    });
    const api =
      claudeModes.querySelector<HTMLInputElement>('input[value="api"]');
    expect(api).not.toBeNull();
    await user.click(api!);
    expect(api).toBeChecked();
    expect(screen.getByLabelText("Claude API Key")).toBeVisible();
  });

  it("presents Google as Gemini with API-only access", async () => {
    const user = userEvent.setup();
    // The fixture's legacy "subscription" value must render as API.
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Connections" }),
    );
    const modes = screen.getByRole("radiogroup", {
      name: "Gemini connection mode",
    });
    const subscription = modes.querySelector<HTMLInputElement>(
      'input[value="subscription"]',
    );
    const api = modes.querySelector<HTMLInputElement>('input[value="api"]');
    expect(subscription).toBeDisabled();
    expect(subscription).not.toBeChecked();
    expect(api).toBeChecked();
    expect(
      screen.getByText(/Gemini connects through the Google API/),
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

    expect(
      await screen.findByRole("heading", { name: "PDF Extraction" }),
    ).toBeVisible();
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
        expect(scrollIntoView).toHaveBeenCalledWith({ block: "center" });
        expect(target.contains(document.activeElement)).toBe(true);
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
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        const result = catalog(args?.provider ?? "local", "cli", "live");
        if (args?.provider === "claude") {
          result.roles = [
            {
              id: "balanced",
              label: "Balanced",
              description: "Stable balanced role",
              model: "claude-live",
            },
          ];
          result.models = [
            {
              id: "claude-live",
              display_name: "Claude Live",
              description: "Discovered model",
              is_default: false,
              supported_efforts: [],
              capabilities: [],
              deprecated: false,
            },
          ];
        }
        return Promise.resolve(result);
      }
      return Promise.resolve();
    });

    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await screen.findByText("Preferred Provider");
    for (const label of [
      "Input assessment",
      "Parallel review",
      "Combine results",
      "Sequential steps",
    ])
      await userEvent.click(screen.getAllByText(label)[0]);
    expect(
      await screen.findByRole("option", {
        name: "Claude Live · Parallel steps",
      }),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "Exact model" })).toHaveLength(
      4,
    );
    expect(
      screen.queryByRole("group", { name: "Stable roles" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: /Balanced/ }),
    ).not.toBeInTheDocument();
  });

  it("surfaces a shell-plugin failure when opening the Ollama site", async () => {
    mockLoad(makeSettings());
    openUrl.mockRejectedValueOnce(new Error("no browser"));
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    const user = userEvent.setup();
    await user.click(
      await screen.findByRole("button", { name: "Connections" }),
    );
    await user.click(
      await screen.findByRole("button", {
        name: "More information about Local server",
      }),
    );
    await user.click(screen.getByRole("link", { name: "ollama.com" }));

    expect(
      await screen.findByText("Could not open ollama.com: no browser"),
    ).toBeVisible();
  });

  it("shows essential explanations beside their controls", async () => {
    mockLoad(makeSettings());
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    expect(
      await screen.findByText(/Used when a workflow has no assigned provider/),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", {
        name: "More information about Preferred Provider",
      }),
    ).not.toBeInTheDocument();
  });

  it("offers system, light, and dark appearance choices", async () => {
    mockLoad(makeSettings());
    const onThemeChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="system"
        onThemeChange={onThemeChange}
        initialSection="general"
      />,
    );
    expect(await screen.findByRole("radio", { name: "System" })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(onThemeChange).toHaveBeenCalledWith("dark");
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

  it("surfaces backend warnings from settings load", async () => {
    mockLoad(makeSettings(), [
      "Settings file has invalid JSON: oops. It was moved to settings.json.corrupt",
    ]);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    expect(
      await screen.findByText(/settings\.json\.corrupt/),
    ).toBeInTheDocument();
  });

  it("automatically saves changes and shows the saved indicator", async () => {
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

    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Preferences save automatically.")).toBeVisible();
    await user.selectOptions(
      screen.getByLabelText("Preferred Provider"),
      "codex",
    );

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), preferred_provider: "codex" },
      }),
    );
    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });

  it("shows an inline autosave error and retries the current settings", async () => {
    const user = userEvent.setup();
    let saveAttempts = 0;
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    invoke.mockImplementation((cmd: string, args?: { provider?: string }) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        return Promise.resolve(
          catalog(args?.provider ?? "local", "cli", "current"),
        );
      }
      if (cmd === "save_settings") {
        saveAttempts += 1;
        return saveAttempts === 1
          ? Promise.reject(new Error("settings file is locked"))
          : Promise.resolve();
      }
      return Promise.resolve();
    });

    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );
    await user.selectOptions(
      await screen.findByLabelText("Preferred Provider"),
      "codex",
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not save settings: settings file is locked",
    );
    await user.click(screen.getAllByRole("button", { name: "Retry" })[0]);
    await waitFor(() => expect(saveAttempts).toBe(2));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
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
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "save_settings") {
        saveCalls += 1;
        return saveCalls === 1 ? firstSave : Promise.resolve();
      }
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

    await user.click(await screen.findByRole("button", { name: "Reviews" }));
    const reconciliation = screen.getByRole("switch", {
      name: /automatic revision reconciliation/i,
    });
    await user.click(reconciliation);
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(true));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), auto_revision_reconciliation: true },
      }),
    );
    await user.click(reconciliation);
    await act(async () => finishFirstSave());

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: makeSettings(),
      }),
    );
    await waitFor(() => expect(onDirtyChange).toHaveBeenLastCalledWith(false));
    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });

  it("does not discover a partial credential before debounced autosave", async () => {
    let claudeRequests = 0;
    invoke.mockImplementation((cmd: string, args?: { provider?: string }) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "get_model_catalog") {
        if (args?.provider === "claude") {
          claudeRequests += 1;
          return Promise.resolve(catalog("claude", "cli", "saved-account"));
        }
        return Promise.resolve(
          catalog(args?.provider ?? "local", "cli", "current"),
        );
      }
      return Promise.resolve();
    });
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await waitFor(() => expect(claudeRequests).toBe(1));
    invoke.mockClear();

    await user.click(screen.getByRole("button", { name: "Connections" }));
    await user.type(screen.getByLabelText("Local API Key"), "s");
    expect(
      invoke.mock.calls.some(([command]) => command === "save_settings"),
    ).toBe(false);
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
    invoke.mockImplementation(
      (
        cmd: string,
        args?: {
          provider?: string;
          refresh?: boolean;
          settings?: Settings;
        },
      ) => {
        if (cmd === "list_engines") return Promise.resolve([]);
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
          return Promise.resolve(
            catalog(args?.provider ?? "local", "cli", "current"),
          );
        }
        if (cmd === "save_settings") return Promise.resolve();
        return Promise.resolve();
      },
    );
    const user = userEvent.setup();
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await waitFor(() => expect(claudeRequests).toBe(1));
    await user.click(screen.getByRole("button", { name: "Connections" }));
    const claudeModes = screen.getByRole("radiogroup", {
      name: "Claude connection mode",
    });
    await user.click(
      claudeModes.querySelector<HTMLInputElement>('input[value="api"]')!,
    );
    await user.type(screen.getByLabelText("Claude API Key"), "sk-complete-key");
    await user.click(
      screen.getByRole("button", { name: "Save Claude API Key" }),
    );
    expect(claudeRequests).toBe(1);

    await waitFor(() => expect(claudeRequests).toBe(2));
    expect(invoke).toHaveBeenCalledWith(
      "get_model_catalog",
      expect.objectContaining({
        provider: "claude",
        refresh: true,
        settings: expect.objectContaining({
          anthropic_api_key: "sk-complete-key",
          claude_access_mode: "api",
        }),
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

  it("previews an exact purge and reports the number actually removed", async () => {
    const user = userEvent.setup();
    let usageCalls = 0;
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
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
    confirmDialog.mockResolvedValueOnce(true);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Data & Storage" }),
    );
    await screen.findByText("8 reports · 7.0 GB");
    await user.click(screen.getByRole("button", { name: "Review cleanup…" }));

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
    expect(confirmDialog.mock.calls[0][0]).toContain(
      "move 3 completed reports (2.5 GB)",
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Moved 3 completed reports to Trash.",
    );
  });

  it("does not ask for destructive confirmation when the preview is empty", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
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
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Data & Storage" }),
    );
    await user.click(screen.getByRole("button", { name: "Review cleanup…" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "No completed reports were beyond the configured limits.",
    );
    expect(confirmDialog).not.toHaveBeenCalled();
    expect(
      invoke.mock.calls.some(([command]) => command === "purge_runs"),
    ).toBe(false);
  });

  it("does not purge when the exact preview is not confirmed", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
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
    confirmDialog.mockResolvedValueOnce(false);
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Data & Storage" }),
    );
    await user.click(screen.getByRole("button", { name: "Review cleanup…" }));
    await waitFor(() => expect(confirmDialog).toHaveBeenCalledTimes(1));
    expect(
      invoke.mock.calls.some(([command]) => command === "purge_runs"),
    ).toBe(false);
  });

  it("stops safely and shows an error when purge preview fails", async () => {
    const user = userEvent.setup();
    invoke.mockImplementation((cmd: string) => {
      if (cmd === "list_engines") return Promise.resolve([]);
      if (cmd === "get_settings") {
        return Promise.resolve({ settings: makeSettings(), warnings: [] });
      }
      if (cmd === "runs_disk_usage") {
        return Promise.resolve({ count: 8, bytes: 7_000_000_000 });
      }
      if (cmd === "preview_purge_runs") {
        return Promise.reject(new Error("history index unavailable"));
      }
      return Promise.reject(new Error(`unexpected command: ${cmd}`));
    });
    render(
      <SettingsPage
        onClose={() => {}}
        theme="light"
        onThemeChange={() => {}}
      />,
    );

    await user.click(
      await screen.findByRole("button", { name: "Data & Storage" }),
    );
    await user.click(screen.getByRole("button", { name: "Review cleanup…" }));

    expect(
      await screen.findByText(
        "The retention preview could not be loaded; no reports were moved: history index unavailable",
      ),
    ).toBeVisible();
    expect(
      invoke.mock.calls.some(([command]) => command === "purge_runs"),
    ).toBe(false);
  });

  it("shows an error state with a working back button when loading fails", async () => {
    invoke.mockRejectedValue(new Error("disk on fire"));
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(
      <SettingsPage onClose={onClose} theme="light" onThemeChange={() => {}} />,
    );

    expect(
      await screen.findByText(/Failed to load settings/),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go back" }));
    expect(onClose).toHaveBeenCalled();
  });
});
