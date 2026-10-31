import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UpdateBanner from "./UpdateBanner";
import type {
  UpdateInfo,
  UpdateInstallState,
  UpdateStatus,
} from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
const unlisten = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

const DISMISS_KEY = "pipeline:update-dismissed-version";

let emitUpdate: (payload: UpdateInfo) => void = () => {};
let emitInstall: (payload: UpdateInstallState) => void = () => {};
let emitStatus: (payload: UpdateStatus) => void = () => {};
const confirmRestart = vi.fn<() => Promise<boolean>>();

function info(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    current: "1.0.0",
    latest: "1.1.0",
    update_available: true,
    release_url: "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    release_name: "v1.1.0",
    published_at: null,
    download_url:
      "https://github.com/mdroste/pipeline/releases/download/v1.1.0/Pipeline-1.1.0-macOS-AppleSilicon.dmg",
    can_install: false,
    checked_at: "2026-10-28T12:00:00Z",
    ...overrides,
  };
}

function status(saved: UpdateInfo | null): UpdateStatus {
  return {
    automatic: true,
    automatic_available: true,
    last_checked_at: saved?.checked_at ?? null,
    info: saved,
    error: null,
  };
}

function installing(
  overrides: Partial<UpdateInstallState> = {},
): UpdateInstallState {
  return {
    phase: "downloading",
    version: "1.1.0",
    downloaded: 0,
    total: null,
    error: null,
    ...overrides,
  };
}

/** Answers `update_status` with `saved`; other commands succeed quietly. */
function respond(
  saved: UpdateStatus | Promise<UpdateStatus> | Error,
  overrides: Record<string, () => unknown> = {},
) {
  invoke.mockImplementation((command: string) => {
    if (command in overrides) return Promise.resolve(overrides[command]());
    if (command === "update_status")
      return saved instanceof Error
        ? Promise.reject(saved)
        : Promise.resolve(saved);
    if (command === "update_install_state") return Promise.resolve(null);
    return Promise.resolve(undefined);
  });
}

function renderBanner() {
  return render(<UpdateBanner confirmRestart={confirmRestart} />);
}

async function settle() {
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_status"));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

describe("UpdateBanner", () => {
  beforeEach(() => {
    invoke.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
    confirmRestart.mockReset();
    confirmRestart.mockResolvedValue(true);
    unlisten.mockReset();
    listen.mockReset();
    listen.mockImplementation(
      (event: string, handler: (event: { payload: never }) => void) => {
        if (event === "update-available")
          emitUpdate = (payload) => handler({ payload: payload as never });
        else if (event === "update-install")
          emitInstall = (payload) => handler({ payload: payload as never });
        else emitStatus = (payload) => handler({ payload: payload as never });
        return Promise.resolve(unlisten);
      },
    );
    localStorage.clear();
  });

  it("reads the saved result instead of checking GitHub on mount", async () => {
    respond(status(info()));
    renderBanner();
    expect(await screen.findByText(/Update available:/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("check_for_update");
    expect(listen).toHaveBeenCalledWith(
      "update-available",
      expect.any(Function),
    );
  });

  it("renders nothing when no update is available", async () => {
    respond(status(info({ update_available: false })));
    const { container } = renderBanner();
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing before the first check", async () => {
    respond(status(null));
    const { container } = renderBanner();
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing when the saved status cannot be read (offline-silent)", async () => {
    respond(new Error("unavailable"));
    const { container } = renderBanner();
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("appears when a scheduled check finds a newer release", async () => {
    respond(status(null));
    renderBanner();
    await settle();
    act(() => emitUpdate(info({ latest: "1.2.0" })));
    expect(await screen.findByText(/v1\.2\.0/)).toBeInTheDocument();
  });

  it("does not replace a live event with an older saved result", async () => {
    let resolveStatus: (value: UpdateStatus) => void = () => {};
    respond(
      new Promise<UpdateStatus>((resolve) => {
        resolveStatus = resolve;
      }),
    );
    renderBanner();
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_status"));
    act(() => emitUpdate(info({ latest: "1.3.0" })));
    await act(async () => resolveStatus(status(info({ latest: "1.2.0" }))));
    expect(await screen.findByText(/v1\.3\.0/)).toBeInTheDocument();
    expect(screen.queryByText(/v1\.2\.0/)).not.toBeInTheDocument();
  });

  it("downloads this platform's installer", async () => {
    respond(status(info()));
    renderBanner();
    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(info().download_url);
  });

  it("opens the release page when no installer is attached", async () => {
    respond(status(info({ download_url: null })));
    renderBanner();
    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    );
  });

  it("opens the release notes", async () => {
    respond(status(info()));
    renderBanner();
    const btn = await screen.findByRole("button", { name: /what’s new/i });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    );
  });

  it("shows a shell-plugin error when the release page cannot be opened", async () => {
    respond(status(info()));
    openUrl.mockRejectedValueOnce(new Error("no system browser"));
    renderBanner();

    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the release page: no system browser",
    );
  });

  it("remembers dismissal for the same version and removes the banner", async () => {
    respond(status(info()));
    renderBanner();
    const dismiss = await screen.findByRole("button", { name: /dismiss/i });

    await userEvent.setup().click(dismiss);

    expect(localStorage.getItem(DISMISS_KEY)).toBe("1.1.0");
    await waitFor(() =>
      expect(screen.queryByText(/Update available:/)).not.toBeInTheDocument(),
    );
  });

  it("suppresses the banner on remount if the same latest version was dismissed", async () => {
    localStorage.setItem(DISMISS_KEY, "1.1.0");
    respond(status(info()));

    const { container } = renderBanner();
    await settle();
    expect(container.firstChild).toBeNull();
    act(() => emitUpdate(info()));
    expect(container.firstChild).toBeNull();
  });

  it("shows the banner again when a newer version ships than the one previously dismissed", async () => {
    localStorage.setItem(DISMISS_KEY, "1.1.0");
    respond(status(info({ latest: "1.2.0" })));

    renderBanner();
    expect(await screen.findByText(/v1\.2\.0/)).toBeInTheDocument();
  });

  it("stops listening on unmount", async () => {
    respond(status(null));
    const { unmount } = renderBanner();
    await settle();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(3);
  });

  describe("installing in the app", () => {
    const installable = () => info({ can_install: true });

    it("starts the install instead of opening a browser", async () => {
      respond(status(installable()));
      renderBanner();
      await userEvent
        .setup()
        .click(await screen.findByRole("button", { name: "Install update" }));
      expect(invoke).toHaveBeenCalledWith("update_install_start");
      expect(openUrl).not.toHaveBeenCalled();
      expect(
        screen.queryByRole("button", { name: "Download" }),
      ).not.toBeInTheDocument();
    });

    it("shows download progress and can cancel back to the offer", async () => {
      respond(status(installable()));
      renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() =>
        emitInstall(installing({ downloaded: 21_000_000, total: 42_000_000 })),
      );
      expect(
        screen.getByText("Downloading version 1.1.0…"),
      ).toBeInTheDocument();
      expect(screen.getByRole("progressbar")).toHaveAttribute(
        "aria-valuenow",
        "50",
      );

      await userEvent
        .setup()
        .click(screen.getByRole("button", { name: "Cancel" }));
      expect(invoke).toHaveBeenCalledWith("update_install_cancel");
      act(() => emitInstall(installing({ phase: "idle", version: null })));
      expect(screen.getByText(/Update available:/)).toBeInTheDocument();
    });

    it("omits the progress bar when the size is unknown", async () => {
      respond(status(installable()));
      renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() => emitInstall(installing({ downloaded: 5_000_000 })));
      expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    });

    it("waits for the researcher before restarting", async () => {
      respond(status(installable()));
      renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() => emitInstall(installing({ phase: "ready" })));
      expect(screen.getByText("Version 1.1.0 is ready.")).toBeInTheDocument();
      expect(invoke).not.toHaveBeenCalledWith("update_install_restart");

      await userEvent
        .setup()
        .click(screen.getByRole("button", { name: "Restart now" }));
      expect(confirmRestart).toHaveBeenCalledTimes(1);
      expect(invoke).toHaveBeenCalledWith("update_install_restart");
    });

    it("stays put when the researcher declines the restart warning", async () => {
      confirmRestart.mockResolvedValue(false);
      respond(status(installable()));
      renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() => emitInstall(installing({ phase: "ready" })));
      await userEvent
        .setup()
        .click(screen.getByRole("button", { name: "Restart now" }));
      expect(invoke).not.toHaveBeenCalledWith("update_install_restart");
      expect(screen.getByText("Version 1.1.0 is ready.")).toBeInTheDocument();
    });

    it("Later hides the banner without dismissing the version", async () => {
      respond(status(installable()));
      const { container } = renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() => emitInstall(installing({ phase: "ready" })));
      await userEvent
        .setup()
        .click(screen.getByRole("button", { name: "Later" }));
      expect(invoke).toHaveBeenCalledWith("update_install_cancel");
      act(() => emitInstall(installing({ phase: "idle", version: null })));
      expect(container.firstChild).toBeNull();
      expect(localStorage.getItem(DISMISS_KEY)).toBeNull();
    });

    it("shows an install started from Settings after a dismissal", async () => {
      localStorage.setItem(DISMISS_KEY, "1.1.0");
      respond(status(installable()));
      const { container } = renderBanner();
      await settle();
      expect(container.firstChild).toBeNull();
      act(() => emitInstall(installing()));
      expect(
        screen.getByText("Downloading version 1.1.0…"),
      ).toBeInTheDocument();
    });

    it("offers a retry and the browser after a failure", async () => {
      respond(status(installable()));
      renderBanner();
      await screen.findByRole("button", { name: "Install update" });
      act(() =>
        emitInstall(
          installing({ phase: "failed", error: "Check your connection." }),
        ),
      );
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Update failed. Check your connection.",
      );

      const user = userEvent.setup();
      await user.click(
        screen.getByRole("button", { name: "Download instead" }),
      );
      expect(openUrl).toHaveBeenCalledWith(info().download_url);
      await user.click(screen.getByRole("button", { name: "Try again" }));
      expect(invoke).toHaveBeenCalledWith("update_install_start");
    });

    it("picks up an install already running when it mounts", async () => {
      respond(status(installable()), {
        update_install_state: () => installing({ phase: "ready" }),
      });
      renderBanner();
      expect(
        await screen.findByText("Version 1.1.0 is ready."),
      ).toBeInTheDocument();
    });

    it("reports why an install could not start", async () => {
      respond(status(installable()), {
        update_install_start: () => Promise.reject("No update is available."),
      });
      renderBanner();
      await userEvent
        .setup()
        .click(await screen.findByRole("button", { name: "Install update" }));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "No update is available.",
      );
    });
  });

  describe("the automatic check setting", () => {
    it("turns checks off from the banner", async () => {
      respond(status(info()), {
        update_set_automatic: () => ({ ...status(info()), automatic: false }),
      });
      renderBanner();
      const box = await screen.findByRole("checkbox", {
        name: "Check automatically",
      });
      expect(box).toBeChecked();

      await userEvent.setup().click(box);
      expect(invoke).toHaveBeenCalledWith("update_set_automatic", {
        enabled: false,
      });
      act(() => emitStatus({ ...status(info()), automatic: false }));
      expect(box).not.toBeChecked();
    });

    it("follows a change made in Settings", async () => {
      respond(status(info()));
      renderBanner();
      const box = await screen.findByRole("checkbox", {
        name: "Check automatically",
      });
      act(() => emitStatus({ ...status(info()), automatic: false }));
      expect(box).not.toBeChecked();
    });

    it("keeps the setting when it cannot be saved", async () => {
      respond(status(info()), {
        update_set_automatic: () => Promise.reject(new Error("read only")),
      });
      renderBanner();
      const box = await screen.findByRole("checkbox", {
        name: "Check automatically",
      });
      await userEvent.setup().click(box);
      expect(await screen.findByRole("alert")).toHaveTextContent(
        "Could not save the update setting.",
      );
      expect(box).toBeChecked();
    });

    it("is hidden in builds that never check by themselves", async () => {
      respond({ ...status(info()), automatic_available: false });
      renderBanner();
      await screen.findByText(/Update available:/);
      expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    });
  });
});
