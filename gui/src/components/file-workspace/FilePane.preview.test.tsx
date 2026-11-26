import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import type { FilePreview } from "../../lib/fileWorkspaceClient";

vi.mock("./SourceEditor", () => ({
  default: ({
    value,
    label,
    onChange,
  }: {
    value: string;
    label: string;
    onChange?: (text: string) => void;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      onChange={(event) => onChange?.(event.target.value)}
    />
  ),
}));
beforeEach(() => localStorage.clear());

it("updates the split preview after typing pauses, and a full preview at once", async () => {
  const read = vi.fn(async (path: string): Promise<FilePreview> => ({
    path,
    hash: "original",
    bytes: 12,
    editable: true,
    truncated: false,
    externalPath: null,
    base64: null,
    mime: "text/markdown",
    text: "# Original",
  }));
  render(
    <FileWorkspace
      adapter={{ id: "split", read, save: vi.fn() }}
      paths={["notes.md"]}
      initial={{ path: "notes.md" }}
    />,
  );
  fireEvent.click(
    await screen.findByRole("button", { name: "Split" }, { timeout: 15_000 }),
  );
  // Without layout, the split shows one pane at a time; the preview pane is
  // hidden but rendered.
  const heading = (name: string) =>
    screen.queryByRole("heading", { name, hidden: true });
  expect(
    await screen.findByRole("heading", { name: "Original", hidden: true }),
  ).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Source editor: notes.md"), {
    target: { value: "# Edited" },
  });
  // The keystroke itself does not render the report again.
  expect(heading("Original")).toBeInTheDocument();
  expect(heading("Edited")).toBeNull();
  expect(
    await screen.findByRole("heading", { name: "Edited", hidden: true }),
  ).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Source editor: notes.md"), {
    target: { value: "# Latest" },
  });
  // Leaving the split shows the current draft without waiting.
  fireEvent.click(screen.getByRole("button", { name: "Preview" }));
  expect(heading("Latest")).toBeInTheDocument();
}, 30_000);
