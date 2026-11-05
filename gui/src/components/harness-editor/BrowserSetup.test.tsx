import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import BrowserSetup from "./BrowserSetup";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
it("prepares an enabled browser without host Node or an extra install click", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "workbench_browser_status"
      ? { installed: false, nodeAvailable: false, platformAvailable: true }
      : { installed: true, nodeAvailable: true, platformAvailable: true },
  );
  render(<BrowserSetup disabled={false} />);
  await screen.findByText(/Browser ready/);
  expect(invoke).toHaveBeenCalledWith("workbench_browser_install");
  expect(screen.queryByText(/Install Node/)).toBeNull();
});
it("shows setup failures and waits for an explicit retry", async () => {
  let attempts = 0;
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "workbench_browser_status")
      return { installed: false, platformAvailable: true };
    if (++attempts === 1) throw new Error("Download interrupted");
    return { installed: true, platformAvailable: true };
  });
  render(<BrowserSetup disabled={false} />);
  await screen.findByRole("alert");
  expect(attempts).toBe(1);
  fireEvent.click(screen.getByRole("button", { name: "Retry browser setup" }));
  await screen.findByText(/Browser ready/);
  expect(attempts).toBe(2);
});
