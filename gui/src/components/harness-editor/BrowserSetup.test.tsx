import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import BrowserSetup from "./BrowserSetup";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
it("downloads the optional browser only after Install browser is selected", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "workbench_browser_status"
      ? { installed: false, nodeAvailable: true, version: "test" }
      : { installed: true, nodeAvailable: true, version: "test" },
  );
  render(<BrowserSetup disabled={false} />);
  await screen.findByRole("button", { name: "Install browser" });
  expect(invoke).not.toHaveBeenCalledWith("workbench_browser_install");
  await vi.waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Install browser" }),
    ).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "Install browser" }));
  await screen.findByText(/Browser installed/);
  expect(invoke).toHaveBeenCalledWith("workbench_browser_install");
});
