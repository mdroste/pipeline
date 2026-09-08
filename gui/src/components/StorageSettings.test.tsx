import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import StorageSettings from "./StorageSettings";

const { invoke, open } = vi.hoisted(() => ({ invoke: vi.fn(), open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

const initial = {
  activeDirectory: "/home/researcher/.pipeline",
  configuredDirectory: "/home/researcher/.pipeline",
  defaultDirectory: "/home/researcher/.pipeline",
  restartRequired: false,
  error: null,
};

describe("StorageSettings", () => {
  beforeEach(() => {
    invoke.mockReset();
    open.mockReset();
    invoke.mockImplementation((command: string) => {
      if (command === "get_storage_settings") return Promise.resolve(initial);
      return Promise.reject(new Error(`Unexpected command ${command}`));
    });
  });

  it("saves a selected folder and distinguishes the active location until restart", async () => {
    const onSavingChange = vi.fn();
    const user = userEvent.setup();
    open.mockResolvedValue("/research/Pipeline");
    invoke.mockImplementation((command: string) => Promise.resolve(command === "set_storage_directory"
      ? { ...initial, configuredDirectory: "/research/Pipeline", restartRequired: true } : initial));
    render(<StorageSettings onSavingChange={onSavingChange} />);
    await user.click(await screen.findByRole("button", { name: "Choose folder…" }));
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ directory: true, multiple: false }));
    expect(invoke).toHaveBeenCalledWith("set_storage_directory", { directory: "/research/Pipeline" });
    expect(screen.getByLabelText("Folder after restart")).toHaveValue("/research/Pipeline");
    expect(screen.getByRole("status")).toHaveTextContent("Quit and reopen Pipeline");
    expect(screen.getByRole("status")).toHaveTextContent(initial.activeDirectory);
    expect(screen.getByText(/Existing data stays in its original folder/)).toBeVisible();
    expect(onSavingChange.mock.calls).toEqual([[true], [false]]);
  });

  it("cancelling the folder picker leaves the setting unchanged", async () => {
    open.mockResolvedValue(null);
    const user = userEvent.setup();
    render(<StorageSettings onSavingChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Choose folder…" }));
    expect(invoke.mock.calls.map(([command]) => command)).toEqual(["get_storage_settings"]);
    expect(screen.getByLabelText("Current folder")).toHaveValue(initial.activeDirectory);
  });

  it("reports a failed save while retaining the current location", async () => {
    const user = userEvent.setup();
    open.mockResolvedValue("/read-only");
    invoke.mockImplementation((command: string) => command === "set_storage_directory"
      ? Promise.reject("Research data folder is not writable") : Promise.resolve(initial));
    render(<StorageSettings onSavingChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Choose folder…" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("not writable");
    expect(screen.getByLabelText("Current folder")).toHaveValue(initial.activeDirectory);
    expect(screen.getByRole("button", { name: "Choose folder…" })).toBeEnabled();
  });

  it("can restore the default when a selected drive is missing", async () => {
    const user = userEvent.setup();
    const missing = { ...initial, activeDirectory: "/missing/drive", configuredDirectory: "/missing/drive", error: "Folder unavailable" };
    invoke.mockImplementation((command: string) => Promise.resolve(command === "set_storage_directory"
      ? { ...missing, configuredDirectory: initial.defaultDirectory, restartRequired: true } : missing));
    render(<StorageSettings onSavingChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Use default folder" }));
    expect(invoke).toHaveBeenCalledWith("set_storage_directory", { directory: initial.defaultDirectory });
    expect(screen.getByLabelText("Folder after restart")).toHaveValue(initial.defaultDirectory);
    expect(screen.getByRole("button", { name: "Open current folder" })).toBeDisabled();
  });

  it("keeps recovery available when the location file cannot be read", async () => {
    invoke.mockResolvedValue({ ...initial, activeDirectory: null, error: "Cannot read data folder setting" });
    render(<StorageSettings onSavingChange={() => {}} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Cannot read");
    expect(screen.getByRole("button", { name: "Use default folder" })).toBeEnabled();
  });

  it("opens the active folder through the backend and surfaces open failures", async () => {
    const user = userEvent.setup();
    render(<StorageSettings onSavingChange={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Open current folder" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_pipeline_dir"));
    expect(screen.getByRole("alert")).toHaveTextContent("Unexpected command open_pipeline_dir");
  });
});
