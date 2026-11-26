import { beforeEach, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPage from "./SettingsPage";
import { catalog, makeSettings } from "../test/settingsFixtures";
import { mockSettingsLoad } from "../test/settingsPageFixtures";
import type { Settings } from "../lib/settingsTypes";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("./DialogService", () => ({ confirmDialog: vi.fn(), notify: vi.fn() }));

beforeEach(() => {
  localStorage.clear();
  invoke.mockReset();
});

function mount() {
  render(
    <SettingsPage
      onClose={() => {}}
      theme="light"
      onThemeChange={() => {}}
      initialSection="providers"
    />,
  );
}

it("uses one default connection control and keeps other providers configurable", async () => {
  const user = userEvent.setup();
  mockSettingsLoad(invoke, {
    ...makeSettings(),
    default_model_chosen: true,
    claude_access_mode: "code_api",
  });
  mount();
  const provider = await screen.findByLabelText("Default provider");
  const connection = screen.getByLabelText("Default connection");
  expect(connection).toHaveValue("claude-code-api");
  expect(
    screen.queryByLabelText("Anthropic connection"),
  ).not.toBeInTheDocument();
  expect(screen.getByLabelText("OpenAI connection")).toBeVisible();

  await user.click(
    screen.getByRole("button", { name: "Change default connection" }),
  );
  expect(connection).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Set up connection" }));
  expect(document.activeElement?.closest("#anthropic-provider")).not.toBeNull();

  await user.selectOptions(provider, "codex");
  expect(screen.queryByLabelText("OpenAI connection")).not.toBeInTheDocument();
  const other = screen.getByLabelText("Anthropic connection");
  expect(other).toHaveValue("claude-code-api");
  await user.selectOptions(other, "anthropic-api");
  expect(provider).toHaveValue("codex");
  await waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        preferred_provider: "codex",
        claude_access_mode: "api",
        codex_access_mode: "subscription",
      }),
    }),
  );
  await user.selectOptions(provider, "claude");
  expect(screen.getByLabelText("Default connection")).toHaveValue(
    "anthropic-api",
  );
  expect(screen.getByLabelText("Claude API Key")).toBeVisible();
});

it.each([
  ["claude-code-api", "code_api"],
  ["anthropic-api", "api"],
] as const)(
  "refreshes the catalog only after saving the %s connection",
  async (id, mode) => {
    const user = userEvent.setup();
    const settings = {
      ...makeSettings(),
      default_model_chosen: true,
      anthropic_api_key: "saved-direct-key",
    };
    mockSettingsLoad(invoke, settings);
    const fallback = invoke.getMockImplementation()!;
    let finishSave = () => {};
    const saving = new Promise<void>((resolve) => {
      finishSave = resolve;
    });
    invoke.mockImplementation(
      (cmd: string, args?: { provider?: string; settings?: Settings }) => {
        if (cmd === "save_settings") return saving;
        if (cmd === "get_model_catalog") {
          const provider = args?.provider ?? "claude";
          const selectedMode =
            args?.settings?.claude_access_mode ?? "subscription";
          const result = catalog(
            provider,
            selectedMode === "api" ? "api" : "cli",
            selectedMode,
          );
          result.models = [
            {
              ...result.models[0],
              id: selectedMode,
              display_name: `${selectedMode} model`,
            },
          ];
          return Promise.resolve(result);
        }
        return fallback(cmd, args);
      },
    );
    mount();
    const model = await screen.findByLabelText("Default model");
    await waitFor(() =>
      expect(
        within(model).getByRole("option", { name: "subscription model" }),
      ).toBeInTheDocument(),
    );
    await user.selectOptions(screen.getByLabelText("Default connection"), id);
    expect(
      within(model).queryByRole("option", { name: "subscription model" }),
    ).not.toBeInTheDocument();
    const updated = expect.objectContaining({
      provider: "claude",
      refresh: true,
      settings: expect.objectContaining({
        claude_access_mode: mode,
        anthropic_api_key: "saved-direct-key",
      }),
    });
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: expect.objectContaining({ claude_access_mode: mode }),
      }),
    );
    expect(invoke).not.toHaveBeenCalledWith("get_model_catalog", updated);
    finishSave();
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_model_catalog", updated),
    );
    await waitFor(() =>
      expect(
        within(model).getByRole("option", { name: `${mode} model` }),
      ).toBeInTheDocument(),
    );
  },
);
