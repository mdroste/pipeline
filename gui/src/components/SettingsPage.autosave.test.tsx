import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
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

describe("SettingsPage autosave", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirmDialog.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
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
    await screen.findByLabelText("Step time limit");

    expect(
      screen.queryByRole("button", { name: "Save" }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Preferences save automatically.")).toBeVisible();
    await user.selectOptions(screen.getByLabelText("Step time limit"), "1800");

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("save_settings", {
        settings: { ...makeSettings(), step_timeout_secs: 1800 },
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
      await screen.findByLabelText("Step time limit"),
      "1800",
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
      name: /compare with the previous report/i,
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
