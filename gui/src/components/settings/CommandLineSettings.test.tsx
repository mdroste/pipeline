import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import CommandLineSettings from "./CommandLineSettings";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const ready = {
  installed: false,
  available: true,
  reason: null,
  path: "/home/user/.pipeline/bin/pipeline-cli",
  command: "'/home/user/.pipeline/bin/pipeline-cli' --help",
  terminal_setup: "export PATH='/home/user/.pipeline/bin':\"$PATH\"",
};
beforeEach(() => invoke.mockReset());

it("installs, repairs and removes the optional command without changing settings", async () => {
  invoke
    .mockResolvedValueOnce(ready)
    .mockResolvedValueOnce({ ...ready, installed: true })
    .mockResolvedValueOnce({ ...ready, installed: true })
    .mockResolvedValueOnce(ready);
  render(<CommandLineSettings />);
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Install command" }),
  );
  expect(invoke).toHaveBeenCalledWith("install_cli_tools");
  expect(await screen.findByLabelText("Try it in a terminal")).toHaveValue(
    ready.command,
  );
  await user.click(screen.getByRole("button", { name: "Repair command" }));
  await user.click(
    await screen.findByRole("button", { name: "Uninstall command" }),
  );
  expect(invoke).toHaveBeenCalledWith("uninstall_cli_tools");
  expect(
    await screen.findByRole("button", { name: "Install command" }),
  ).toBeEnabled();
});

it("keeps an installation failure visible and allows retry", async () => {
  invoke
    .mockResolvedValueOnce(ready)
    .mockRejectedValueOnce("Refusing to replace an unowned command file")
    .mockResolvedValueOnce({ ...ready, installed: true });
  render(<CommandLineSettings />);
  const user = userEvent.setup();
  await user.click(
    await screen.findByRole("button", { name: "Install command" }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent("unowned");
  await user.click(screen.getByRole("button", { name: "Install command" }));
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Command installed",
  );
});

it("recovers a failed status check and disables actions while running", async () => {
  let finish!: (value: typeof ready) => void;
  invoke
    .mockRejectedValueOnce("Cannot read command status")
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
  render(<CommandLineSettings />);
  const user = userEvent.setup();
  await screen.findByRole("alert");
  await user.click(
    screen.getByRole("button", { name: "Check command status" }),
  );
  expect(
    screen.getByRole("button", { name: "Check command status" }),
  ).toBeDisabled();
  await act(async () => finish(ready));
  expect(
    await screen.findByRole("button", { name: "Install command" }),
  ).toBeEnabled();
});
