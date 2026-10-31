import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import ShellSetup from "./ShellSetup";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
it("downloads the optional command engine only after Install command engine is selected", async () => {
  vi.mocked(invoke).mockImplementation(async (command) =>
    command === "workbench_shell_status"
      ? {
          installed: false,
          nodeAvailable: true,
          platformAvailable: true,
          version: "test",
        }
      : {
          installed: true,
          nodeAvailable: true,
          platformAvailable: true,
          version: "test",
        },
  );
  render(<ShellSetup disabled={false} />);
  await screen.findByRole("button", { name: "Install command engine" });
  expect(invoke).not.toHaveBeenCalledWith("workbench_shell_install");
  await vi.waitFor(() =>
    expect(
      screen.getByRole("button", { name: "Install command engine" }),
    ).toBeEnabled(),
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Install command engine" }),
  );
  await screen.findByText(/Command engine installed/);
  expect(invoke).toHaveBeenCalledWith("workbench_shell_install");
});
