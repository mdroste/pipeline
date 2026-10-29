import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UpdateBanner from "./UpdateBanner";
import type { UpdateInfo, UpdateStatus } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
const unlisten = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

const DISMISS_KEY = "pipeline:update-dismissed-version";

let emitUpdate: (payload: UpdateInfo) => void = () => {};

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
    unlisten.mockReset();
    listen.mockReset();
    listen.mockImplementation(
      (_event: string, handler: (event: { payload: UpdateInfo }) => void) => {
        emitUpdate = (payload) => handler({ payload });
        return Promise.resolve(unlisten);
      },
    );
    localStorage.clear();
  });

  it("reads the saved result instead of checking GitHub on mount", async () => {
    invoke.mockResolvedValueOnce(status(info()));
    render(<UpdateBanner />);
    expect(await screen.findByText(/Update available:/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("check_for_update");
    expect(listen).toHaveBeenCalledWith(
      "update-available",
      expect.any(Function),
    );
  });

  it("renders nothing when no update is available", async () => {
    invoke.mockResolvedValueOnce(status(info({ update_available: false })));
    const { container } = render(<UpdateBanner />);
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing before the first check", async () => {
    invoke.mockResolvedValueOnce(status(null));
    const { container } = render(<UpdateBanner />);
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing when the saved status cannot be read (offline-silent)", async () => {
    invoke.mockRejectedValueOnce(new Error("unavailable"));
    const { container } = render(<UpdateBanner />);
    await settle();
    expect(container.firstChild).toBeNull();
  });

  it("appears when a scheduled check finds a newer release", async () => {
    invoke.mockResolvedValueOnce(status(null));
    render(<UpdateBanner />);
    await settle();
    act(() => emitUpdate(info({ latest: "1.2.0" })));
    expect(await screen.findByText(/v1\.2\.0/)).toBeInTheDocument();
  });

  it("does not replace a live event with an older saved result", async () => {
    let resolveStatus: (value: UpdateStatus) => void = () => {};
    invoke.mockReturnValueOnce(
      new Promise<UpdateStatus>((resolve) => {
        resolveStatus = resolve;
      }),
    );
    render(<UpdateBanner />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("update_status"));
    act(() => emitUpdate(info({ latest: "1.3.0" })));
    await act(async () => resolveStatus(status(info({ latest: "1.2.0" }))));
    expect(await screen.findByText(/v1\.3\.0/)).toBeInTheDocument();
    expect(screen.queryByText(/v1\.2\.0/)).not.toBeInTheDocument();
  });

  it("downloads this platform's installer", async () => {
    invoke.mockResolvedValueOnce(status(info()));
    render(<UpdateBanner />);
    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(info().download_url);
  });

  it("opens the release page when no installer is attached", async () => {
    invoke.mockResolvedValueOnce(status(info({ download_url: null })));
    render(<UpdateBanner />);
    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    );
  });

  it("opens the release notes", async () => {
    invoke.mockResolvedValueOnce(status(info()));
    render(<UpdateBanner />);
    const btn = await screen.findByRole("button", { name: /what’s new/i });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    );
  });

  it("shows a shell-plugin error when the release page cannot be opened", async () => {
    invoke.mockResolvedValueOnce(status(info()));
    openUrl.mockRejectedValueOnce(new Error("no system browser"));
    render(<UpdateBanner />);

    const btn = await screen.findByRole("button", { name: "Download" });
    await userEvent.setup().click(btn);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the release page: no system browser",
    );
  });

  it("remembers dismissal for the same version and removes the banner", async () => {
    invoke.mockResolvedValueOnce(status(info()));
    render(<UpdateBanner />);
    const dismiss = await screen.findByRole("button", { name: /dismiss/i });

    await userEvent.setup().click(dismiss);

    expect(localStorage.getItem(DISMISS_KEY)).toBe("1.1.0");
    await waitFor(() =>
      expect(screen.queryByText(/Update available:/)).not.toBeInTheDocument(),
    );
  });

  it("suppresses the banner on remount if the same latest version was dismissed", async () => {
    localStorage.setItem(DISMISS_KEY, "1.1.0");
    invoke.mockResolvedValueOnce(status(info()));

    const { container } = render(<UpdateBanner />);
    await settle();
    expect(container.firstChild).toBeNull();
    act(() => emitUpdate(info()));
    expect(container.firstChild).toBeNull();
  });

  it("shows the banner again when a newer version ships than the one previously dismissed", async () => {
    localStorage.setItem(DISMISS_KEY, "1.1.0");
    invoke.mockResolvedValueOnce(status(info({ latest: "1.2.0" })));

    render(<UpdateBanner />);
    expect(await screen.findByText(/v1\.2\.0/)).toBeInTheDocument();
  });

  it("stops listening on unmount", async () => {
    invoke.mockResolvedValueOnce(status(null));
    const { unmount } = render(<UpdateBanner />);
    await settle();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
