import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import WindowControls from "./WindowControls";

const appWindow = vi.hoisted(() => ({
  close: vi.fn(),
  isMaximized: vi.fn(),
  minimize: vi.fn(),
  onResized: vi.fn(),
  toggleMaximize: vi.fn(),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => appWindow,
}));

describe("WindowControls", () => {
  const unlisten = vi.fn();

  beforeEach(() => {
    vi.resetAllMocks();
    appWindow.close.mockResolvedValue(undefined);
    appWindow.isMaximized.mockResolvedValue(false);
    appWindow.minimize.mockResolvedValue(undefined);
    appWindow.onResized.mockResolvedValue(unlisten);
    appWindow.toggleMaximize.mockResolvedValue(undefined);
  });

  it("minimizes, maximizes, and closes the window", async () => {
    const user = userEvent.setup();
    render(<WindowControls />);

    await user.click(screen.getByRole("button", { name: "Minimize" }));
    await user.click(screen.getByRole("button", { name: "Maximize" }));
    await user.click(screen.getByRole("button", { name: "Close" }));

    expect(appWindow.minimize).toHaveBeenCalledTimes(1);
    expect(appWindow.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(appWindow.close).toHaveBeenCalledTimes(1);
  });

  it("offers Restore while the window is maximized", async () => {
    render(<WindowControls />);
    await waitFor(() => expect(appWindow.onResized).toHaveBeenCalledTimes(1));
    expect(
      await screen.findByRole("button", { name: "Maximize" }),
    ).toBeInTheDocument();

    appWindow.isMaximized.mockResolvedValue(true);
    const resized = appWindow.onResized.mock.calls[0][0] as () => void;
    await act(async () => resized());

    expect(
      await screen.findByRole("button", { name: "Restore" }),
    ).toBeInTheDocument();
  });

  it("stops watching the window when removed", async () => {
    const { unmount } = render(<WindowControls />);
    await waitFor(() => expect(appWindow.onResized).toHaveBeenCalledTimes(1));
    await act(async () => {});

    unmount();

    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
