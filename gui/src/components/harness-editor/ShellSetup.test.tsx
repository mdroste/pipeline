import { render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import ShellSetup from "./ShellSetup";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
it("prepares commands with a managed runtime", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "workbench_shell_status"
      ? { installed: false, nodeAvailable: false, platformAvailable: true }
      : { installed: true, platformAvailable: true },
  );
  render(<ShellSetup disabled={false} />);
  await screen.findByText(/Commands ready/);
  expect(invoke).toHaveBeenCalledWith("workbench_shell_install");
});
it("does not attempt unsupported installation", async () => {
  vi.mocked(invoke).mockClear();
  vi.mocked(invoke).mockResolvedValue({
    installed: false,
    platformAvailable: false,
  });
  render(<ShellSetup disabled={false} />);
  await screen.findByText(/General commands require/);
  expect(invoke).not.toHaveBeenCalledWith("workbench_shell_install");
});
it("waits until settings are saved before starting setup", async () => {
  vi.mocked(invoke).mockClear();
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "workbench_shell_status"
      ? { installed: false, platformAvailable: true }
      : { installed: true, platformAvailable: true },
  );
  const view = render(<ShellSetup disabled={true} />);
  await screen.findByText(/downloads and prepares/);
  expect(invoke).not.toHaveBeenCalledWith("workbench_shell_install");
  view.rerender(<ShellSetup disabled={false} />);
  await screen.findByText(/Commands ready/);
});
