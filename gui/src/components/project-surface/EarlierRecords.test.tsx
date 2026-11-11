import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
const client = vi.hoisted(() => ({
  earlierRecords: vi.fn(),
  exportEarlierRecords: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);
vi.mock("../../lib/projectClient", () => ({ projectClient: client }));

import EarlierRecords from "./EarlierRecords";

beforeEach(() => {
  vi.clearAllMocks();
});

it("shows nothing to a project without earlier records", async () => {
  client.earlierRecords.mockResolvedValue({ kinds: [], total: 0 });
  const { container } = render(
    <EarlierRecords workspaceId="ws" root="/paper" reportError={vi.fn()} />,
  );
  await vi.waitFor(() => expect(client.earlierRecords).toHaveBeenCalled());
  expect(container).toBeEmptyDOMElement();
});

it("exports into the chosen folder and reports where the files went", async () => {
  client.earlierRecords.mockResolvedValue({
    kinds: [
      { kind: "theory", label: "Theory notes", count: 2 },
      { kind: "claim", label: "Claims and evidence", count: 1 },
    ],
    total: 3,
  });
  dialog.open.mockResolvedValue("/paper");
  client.exportEarlierRecords.mockResolvedValue({
    directory: "/paper/Earlier research records",
    files: ["README.md", "theory-notes.md", "claims-and-evidence.md"],
    records: 3,
    notesCreated: 1,
  });
  render(
    <EarlierRecords workspaceId="ws" root="/paper" reportError={vi.fn()} />,
  );
  expect(await screen.findByText(/This project has 3 records/)).toBeVisible();
  expect(screen.getByText("Theory notes · 2")).toBeVisible();
  await userEvent.click(
    screen.getByRole("button", { name: "Export as Markdown…" }),
  );
  expect(dialog.open).toHaveBeenCalledWith(
    expect.objectContaining({ directory: true, defaultPath: "/paper" }),
  );
  expect(client.exportEarlierRecords).toHaveBeenCalledWith("ws", "/paper");
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Saved 3 files to /paper/Earlier research records. 1 abandoned approach is now a note.",
  );
});

it("does nothing when the folder dialog is cancelled", async () => {
  client.earlierRecords.mockResolvedValue({
    kinds: [{ kind: "decision", label: "Decisions", count: 1 }],
    total: 1,
  });
  dialog.open.mockResolvedValue(null);
  render(<EarlierRecords workspaceId="ws" root={null} reportError={vi.fn()} />);
  await userEvent.click(
    await screen.findByRole("button", { name: "Export as Markdown…" }),
  );
  expect(client.exportEarlierRecords).not.toHaveBeenCalled();
});
