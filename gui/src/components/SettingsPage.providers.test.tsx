import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPageComponent from "./SettingsPage";
import type { ComponentProps } from "react";
// Existing Review behavior tests enter Reviews explicitly after the navigation redesign.
const SettingsPage = (props: ComponentProps<typeof SettingsPageComponent>) => (
  <SettingsPageComponent initialSection="workflow" {...props} />
);
import { makeSettings, catalog } from "../test/settingsFixtures";
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

describe("SettingsPage providers", () => {
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
});
