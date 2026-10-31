import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import SettingsPageComponent from "./SettingsPage";
import type { ComponentProps } from "react";
// Existing Review behavior tests enter Reviews explicitly after the navigation redesign.
const SettingsPage = (props: ComponentProps<typeof SettingsPageComponent>) => (
  <SettingsPageComponent initialSection="workflow" {...props} />
);
import { makeSettings } from "../test/settingsFixtures";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));
vi.mock("./DialogService", () => ({ confirmDialog, notify: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

describe("SettingsPage history", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
    confirmDialog.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
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
    await user.click(screen.getByRole("button", { name: "Preview cleanup…" }));

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
    await user.click(screen.getByRole("button", { name: "Preview cleanup…" }));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "No reports are over these limits.",
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
    await user.click(screen.getByRole("button", { name: "Preview cleanup…" }));
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
    await user.click(screen.getByRole("button", { name: "Preview cleanup…" }));

    expect(
      await screen.findByText(
        "Could not preview cleanup. No reports were moved. history index unavailable",
      ),
    ).toBeVisible();
    expect(
      invoke.mock.calls.some(([command]) => command === "purge_runs"),
    ).toBe(false);
  });
});
