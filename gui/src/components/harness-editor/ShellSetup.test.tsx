import { fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import ShellSetup from "./ShellSetup";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

it("checks native commands without installing a runtime", async () => {
  vi.mocked(invoke).mockResolvedValue({ available: true, reason: null });
  render(<ShellSetup disabled={false} />);
  await screen.findByText("Commands ready. Included with Pipeline.");
  expect(invoke).toHaveBeenCalledTimes(1);
  expect(invoke).toHaveBeenCalledWith("workbench_shell_status");
});

it("shows the platform's explanation and allows another check", async () => {
  vi.mocked(invoke)
    .mockResolvedValueOnce({ available: false, reason: "Update Windows." })
    .mockResolvedValueOnce({ available: true, reason: null });
  render(<ShellSetup disabled={false} />);
  await screen.findByText("Update Windows.");
  fireEvent.click(screen.getByRole("button", { name: "Check again" }));
  await screen.findByText("Commands ready. Included with Pipeline.");
});

it("allows retrying a failed availability check", async () => {
  vi.mocked(invoke)
    .mockRejectedValueOnce(new Error("Unavailable"))
    .mockResolvedValueOnce({ available: true, reason: null });
  render(<ShellSetup disabled={false} />);
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: "Check again" }));
  await screen.findByText("Commands ready. Included with Pipeline.");
});
