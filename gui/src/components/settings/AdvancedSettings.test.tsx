import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AdvancedSettings from "./AdvancedSettings";
import { version as appVersion } from "../../../package.json";
import { makeSettings } from "../../test/settingsFixtures";
import type { UpdateInfo, UpdateStatus } from "../../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => Promise.resolve(appVersion),
}));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));
vi.mock("../../lib/projectClient", () => ({
  projectClient: { capabilities: () => Promise.reject(new Error("n/a")) },
}));

const available: UpdateInfo = {
  current: appVersion,
  latest: "1.3.0",
  update_available: true,
  release_url: "https://github.com/mdroste/pipeline/releases/tag/v1.3.0",
  release_name: "Pipeline v1.3.0",
  published_at: null,
  download_url:
    "https://github.com/mdroste/pipeline/releases/download/v1.3.0/Pipeline-1.3.0-Windows.exe",
  checked_at: "2026-10-28T12:00:00Z",
};

function status(overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    automatic: true,
    automatic_available: true,
    last_checked_at: null,
    info: null,
    error: null,
    ...overrides,
  };
}

function respond(
  handlers: Record<string, (args?: Record<string, unknown>) => unknown>,
) {
  invoke.mockImplementation(
    (command: string, args?: Record<string, unknown>) =>
      command in handlers
        ? Promise.resolve().then(() => handlers[command](args))
        : Promise.reject(new Error(`unexpected ${command}`)),
  );
}

function renderSettings() {
  return render(
    <AdvancedSettings settings={makeSettings()} setSettings={vi.fn()} />,
  );
}

describe("AdvancedSettings updates", () => {
  beforeEach(() => {
    invoke.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
  });

  it("saves the automatic-check preference", async () => {
    const user = userEvent.setup();
    respond({
      update_status: () => status(),
      update_set_automatic: (args) =>
        status({ automatic: args?.enabled as boolean }),
    });
    renderSettings();

    const toggle = await screen.findByRole("switch", {
      name: "Check for updates automatically",
    });
    expect(toggle).toHaveAttribute("aria-checked", "true");
    await user.click(toggle);

    expect(invoke).toHaveBeenCalledWith("update_set_automatic", {
      enabled: false,
    });
    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-checked", "false"),
    );
  });

  it("explains that unofficial builds check only on request", async () => {
    respond({ update_status: () => status({ automatic_available: false }) });
    renderSettings();
    expect(
      await screen.findByText(/not installed from an official release/),
    ).toBeInTheDocument();
  });

  it("reports an unreadable preference", async () => {
    respond({
      update_status: () => status({ automatic: false, error: "damaged" }),
    });
    renderSettings();
    expect(
      await screen.findByText(/update setting could not be read/),
    ).toBeInTheDocument();
  });

  it("offers the saved update without a new check", async () => {
    const user = userEvent.setup();
    respond({
      update_status: () =>
        status({ info: available, last_checked_at: available.checked_at }),
    });
    renderSettings();

    expect(
      await screen.findByText("Version 1.3.0 is available."),
    ).toBeInTheDocument();
    expect(screen.getByText(/Last checked/)).toBeInTheDocument();
    await user.click(
      screen.getByRole("button", { name: "Download version 1.3.0" }),
    );
    expect(openUrl).toHaveBeenCalledWith(available.download_url);
    await user.click(
      screen.getByRole("button", { name: "View release notes" }),
    );
    expect(openUrl).toHaveBeenCalledWith(available.release_url);
    expect(invoke).not.toHaveBeenCalledWith("check_for_update");
  });

  it("reports an up-to-date manual check", async () => {
    const user = userEvent.setup();
    respond({
      update_status: () => status(),
      check_for_update: () => ({
        ...available,
        latest: appVersion,
        update_available: false,
      }),
    });
    renderSettings();

    await user.click(
      await screen.findByRole("button", { name: "Check for updates" }),
    );
    expect(await screen.findByText("You’re up to date.")).toBeInTheDocument();
  });

  it("shows the backend's explanation when a check fails", async () => {
    const user = userEvent.setup();
    const message =
      "GitHub is limiting update checks from your network. Try again in about 40 minutes.";
    invoke.mockImplementation((command: string) =>
      command === "update_status"
        ? Promise.resolve(status())
        : Promise.reject(message),
    );
    renderSettings();

    await user.click(
      await screen.findByRole("button", { name: "Check for updates" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
  });
});
