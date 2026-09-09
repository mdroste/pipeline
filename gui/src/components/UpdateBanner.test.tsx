import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UpdateBanner from "./UpdateBanner";
import type { UpdateInfo } from "../lib/types";

const invoke = vi.hoisted(() => vi.fn());
const openUrl = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: openUrl }));

const DISMISS_KEY = "pipeline:update-dismissed-version";

function info(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    current: "1.0.0",
    latest: "1.1.0",
    update_available: true,
    release_url: "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    release_name: "v1.1.0",
    published_at: null,
    ...overrides,
  };
}

describe("UpdateBanner", () => {
  beforeEach(() => {
    invoke.mockReset();
    openUrl.mockReset();
    openUrl.mockResolvedValue(undefined);
    localStorage.clear();
  });

  it("renders nothing when no update is available", async () => {
    invoke.mockResolvedValueOnce(info({ update_available: false }));
    const { container } = render(<UpdateBanner />);
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("check_for_update"),
    );
    // After the async check resolves, still nothing rendered.
    await new Promise((r) => setTimeout(r, 0));
    expect(container.firstChild).toBeNull();
  });

  it("renders nothing when the backend check fails (offline-silent)", async () => {
    invoke.mockRejectedValueOnce(new Error("network down"));
    const { container } = render(<UpdateBanner />);
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 0));
    expect(container.firstChild).toBeNull();
  });

  it("renders the banner with current and latest versions when an update is available", async () => {
    invoke.mockResolvedValueOnce(info());
    render(<UpdateBanner />);
    expect(await screen.findByText(/Update available:/)).toBeInTheDocument();
    expect(screen.getByText(/v1\.0\.0/)).toBeInTheDocument();
    expect(screen.getByText(/v1\.1\.0/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /view release/i }),
    ).toBeInTheDocument();
  });

  it("opens the release URL via the shell plugin when 'View release' is clicked", async () => {
    invoke.mockResolvedValueOnce(info());
    render(<UpdateBanner />);
    const btn = await screen.findByRole("button", { name: /view release/i });
    await userEvent.setup().click(btn);
    expect(openUrl).toHaveBeenCalledWith(
      "https://github.com/mdroste/pipeline/releases/tag/v1.1.0",
    );
  });

  it("shows a shell-plugin error when the release page cannot be opened", async () => {
    invoke.mockResolvedValueOnce(info());
    openUrl.mockRejectedValueOnce(new Error("no system browser"));
    render(<UpdateBanner />);

    const btn = await screen.findByRole("button", { name: /view release/i });
    await userEvent.setup().click(btn);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the release page: no system browser",
    );
  });

  it("remembers dismissal for the same version and removes the banner", async () => {
    invoke.mockResolvedValueOnce(info());
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
    invoke.mockResolvedValueOnce(info());

    const { container } = render(<UpdateBanner />);
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 0));
    expect(container.firstChild).toBeNull();
  });

  it("shows the banner again when a newer version ships than the one previously dismissed", async () => {
    localStorage.setItem(DISMISS_KEY, "1.1.0");
    invoke.mockResolvedValueOnce(info({ latest: "1.2.0" }));

    render(<UpdateBanner />);
    expect(await screen.findByText(/v1\.2\.0/)).toBeInTheDocument();
  });
});
