import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterAll, beforeAll, beforeEach, expect, it, vi } from "vitest";
import WorkspaceReleasePanel from "./WorkspaceReleasePanel";

const mocks = vi.hoisted(() => ({
  inspectResearchArchive: vi.fn(),
  importResearchArchive: vi.fn(),
  exportResearchArchive: vi.fn(),
  missingBackupFiles: vi.fn(),
  performanceBudgets: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
  confirmDialog: vi.fn(),
  notify: vi.fn(),
}));
vi.mock("../lib/workbench/releaseClient", () => ({ releaseClient: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
  save: mocks.save,
}));
vi.mock("./DialogService", () => ({
  confirmDialog: mocks.confirmDialog,
  notify: mocks.notify,
}));

// jsdom has no modal dialog support; mirror the open attribute it toggles.
const original = {
  showModal: HTMLDialogElement.prototype.showModal,
  close: HTMLDialogElement.prototype.close,
};
beforeAll(() => {
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
});
afterAll(() => {
  HTMLDialogElement.prototype.showModal = original.showModal;
  HTMLDialogElement.prototype.close = original.close;
});

const onError = vi.fn();
const run = (action: () => Promise<void>) =>
  void action().catch((cause: Error) => onError(cause.message));
const panel = () => (
  <WorkspaceReleasePanel
    view="backup"
    selectedPaper={null}
    busy={false}
    onAction={run}
    onError={onError}
  />
);

beforeEach(() => {
  vi.clearAllMocks();
  mocks.performanceBudgets.mockResolvedValue([]);
  mocks.missingBackupFiles.mockResolvedValue([]);
  mocks.open.mockResolvedValue("/backups/research.pwrx");
  mocks.inspectResearchArchive.mockResolvedValue({
    workspaceRoots: ["/old/paper-a", "/old/paper-b"],
    portabilityNote: "Command profiles must be tested again.",
  });
});

it("maps archived project folders in a form instead of a native prompt", async () => {
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.importResearchArchive.mockRejectedValue(
    new Error("stop before reload"),
  );
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Restore research backup" }),
  );
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalled());
  expect(mocks.confirmDialog.mock.calls[0][0]).toContain("/old/paper-a");

  mocks.open.mockResolvedValueOnce("/new/paper-a");
  fireEvent.click(
    await screen.findByRole("button", {
      name: "Choose folder for /old/paper-a",
    }),
  );
  const first = screen.getByRole("textbox", {
    name: "Folder on this computer for /old/paper-a",
  });
  await waitFor(() => expect(first).toHaveValue("/new/paper-a"));
  expect(mocks.open).toHaveBeenLastCalledWith({
    directory: true,
    multiple: false,
  });

  fireEvent.click(screen.getByRole("button", { name: "Restore" }));
  await waitFor(() =>
    expect(mocks.importResearchArchive).toHaveBeenCalledWith(
      "/backups/research.pwrx",
      { "/old/paper-a": "/new/paper-a", "/old/paper-b": null },
    ),
  );
  expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  await waitFor(() =>
    expect(onError).toHaveBeenCalledWith("stop before reload"),
  );
});

it("rejects a relative folder path and lets the user cancel the restore", async () => {
  mocks.confirmDialog.mockResolvedValue(true);
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Restore research backup" }),
  );
  const first = await screen.findByRole("textbox", {
    name: "Folder on this computer for /old/paper-a",
  });
  fireEvent.change(first, { target: { value: "paper-a" } });
  fireEvent.click(screen.getByRole("button", { name: "Restore" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Enter a full folder path",
  );
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Restore" })).toBeNull(),
  );
  expect(mocks.importResearchArchive).not.toHaveBeenCalled();
});

it("stops before choosing folders when the restore warning is declined", async () => {
  mocks.confirmDialog.mockResolvedValue(false);
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Restore research backup" }),
  );
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalled());
  expect(screen.queryByRole("button", { name: "Restore" })).toBeNull();
  expect(mocks.importResearchArchive).not.toHaveBeenCalled();
});

it("reports a finished export without a native alert", async () => {
  mocks.save.mockResolvedValue("/backups/out.pwrx");
  mocks.exportResearchArchive.mockResolvedValue({
    workspaceCount: 2,
    sessionCount: 3,
    blobCount: 4,
    portabilityNote: "Folders are not included.",
    omittedFiles: [],
  });
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Export research backup" }),
  );
  await waitFor(() =>
    expect(mocks.notify).toHaveBeenCalledWith(
      expect.stringContaining("Exported 2 projects"),
      "success",
    ),
  );
  expect(mocks.confirmDialog).not.toHaveBeenCalled();
  expect(mocks.exportResearchArchive).toHaveBeenCalledWith(
    "/backups/out.pwrx",
    [],
  );
});

const lost = [
  { description: "Paper “Wages”", reference: "workspace://blobs/a.pdf" },
  { description: "Dataset “Survey”", reference: "workspace://blobs/b.csv" },
];

it("asks before leaving out missing files and reports what it left out", async () => {
  mocks.missingBackupFiles.mockResolvedValue(lost);
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.save.mockResolvedValue("/backups/out.pwrx");
  mocks.exportResearchArchive.mockResolvedValue({
    workspaceCount: 1,
    sessionCount: 1,
    blobCount: 3,
    portabilityNote: "Folders are not included.",
    omittedFiles: ["Dataset “Survey”", "Paper “Wages”"],
  });
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Export research backup" }),
  );
  await waitFor(() =>
    expect(mocks.exportResearchArchive).toHaveBeenCalledWith(
      "/backups/out.pwrx",
      ["workspace://blobs/a.pdf", "workspace://blobs/b.csv"],
    ),
  );
  const [question, options] = mocks.confirmDialog.mock.calls[0];
  expect(question).toContain("Paper “Wages”");
  expect(question).toContain("Dataset “Survey”");
  expect(options.confirmLabel).toBe("Back up without them");
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalledTimes(2));
  const [report, reportOptions] = mocks.confirmDialog.mock.calls[1];
  expect(report).toContain("Left out because their files were missing");
  expect(report).toContain("Paper “Wages”");
  expect(reportOptions.acknowledgeOnly).toBe(true);
  expect(mocks.notify).not.toHaveBeenCalled();
});

it("saves nothing when leaving out missing files is declined", async () => {
  mocks.missingBackupFiles.mockResolvedValue(lost);
  mocks.confirmDialog.mockResolvedValue(false);
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Export research backup" }),
  );
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalled());
  expect(mocks.save).not.toHaveBeenCalled();
  expect(mocks.exportResearchArchive).not.toHaveBeenCalled();
});

it("warns before restoring a backup with left-out files and lists them after", async () => {
  mocks.inspectResearchArchive.mockResolvedValue({
    workspaceRoots: [],
    portabilityNote: "Command profiles must be tested again.",
    omittedFileCount: 2,
  });
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.importResearchArchive.mockResolvedValue({
    workspaceCount: 1,
    sessionCount: 1,
    blobCount: 3,
    portabilityNote: "Command profiles must be tested again.",
    omittedFiles: ["Paper “Wages”"],
  });
  const reload = vi.fn();
  const location = window.location;
  Object.defineProperty(window, "location", {
    configurable: true,
    value: { ...location, reload },
  });
  render(panel());
  fireEvent.click(
    screen.getByRole("button", { name: "Restore research backup" }),
  );
  await waitFor(() => expect(reload).toHaveBeenCalled());
  Object.defineProperty(window, "location", {
    configurable: true,
    value: location,
  });
  expect(mocks.confirmDialog.mock.calls[0][0]).toContain(
    "made without 2 files",
  );
  const [report, options] = mocks.confirmDialog.mock.calls[1];
  expect(report).toContain("Paper “Wages”");
  expect(options.acknowledgeOnly).toBe(true);
});
