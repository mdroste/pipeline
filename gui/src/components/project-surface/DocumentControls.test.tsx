import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import DocumentControls from "./DocumentControls";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";
import type { PaperWithRevision } from "../../lib/workbench/researchTypes";
const api = vi.hoisted(() => ({
  documentVersions: vi.fn(),
  updateDocument: vi.fn(),
  read: vi.fn(),
}));
vi.mock("../../lib/workbench/researchClient", () => ({ researchClient: api }));
vi.mock("../../lib/projectClient", () => ({ projectClient: api }));
const document = (
  id: string,
  title: string,
  kind: "paper" | "slides",
  archived = false,
): PaperWithRevision => ({
  paper: {
    id,
    workspaceId: "ws",
    title,
    role: kind === "paper" ? "manuscript" : "other",
    documentType: kind,
    archived,
    pinned: false,
    currentRevisionId: `${id}-2`,
    updatedAt: "2026-11-16T12:00:00Z",
    createdAt: "2026-11-01T12:00:00Z",
  },
  revision: {
    id: `${id}-2`,
    paperId: id,
    capturedAt: "2026-11-16T12:00:00Z",
  } as PaperWithRevision["revision"],
});
const docs = [
  document("paper", "Main paper", "paper"),
  document("seminar", "Seminar deck", "slides"),
  document("conference", "Conference deck", "slides"),
  document("old", "Last year's talk", "slides", true),
];
const error = vi.fn();
function Harness() {
  const [revisionId, onSelect] = useState<string | null>("seminar-2");
  return (
    <>
      <output>{revisionId}</output>
      <DocumentControls
        data={{ ...makeProjectHome(), papers: docs }}
        workspaceId="ws"
        revisionId={revisionId}
        onSelect={onSelect}
        run={async (work) => {
          await work();
        }}
        reportError={error}
        onImport={vi.fn()}
      />
    </>
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  api.documentVersions.mockImplementation(async (_ws, id) => [
    { id: `${id}-2`, paperId: id, capturedAt: "2026-11-16T12:00:00Z" },
    { id: `${id}-1`, paperId: id, capturedAt: "2026-11-01T12:00:00Z" },
  ]);
  api.read.mockImplementation(async (_ws, id: string) => ({
    revision: { id, paperId: id.split("-")[0] },
  }));
  api.updateDocument.mockResolvedValue(docs[1]);
});
it("browses and filters multiple decks without listing archived documents by default", async () => {
  render(<Harness />);
  fireEvent.click(screen.getByRole("button", { name: "Browse documents" }));
  const browser = screen.getByRole("region", { name: "Document browser" });
  fireEvent.change(screen.getByLabelText("Filter document type"), {
    target: { value: "slides" },
  });
  expect(
    within(browser).queryByRole("button", { name: /Main paper/ }),
  ).toBeNull();
  expect(within(browser).getAllByRole("button")).toHaveLength(2);
  fireEvent.change(screen.getByLabelText("Search documents"), {
    target: { value: "conference" },
  });
  fireEvent.click(
    within(browser).getByRole("button", { name: /Conference deck/ }),
  );
  expect(screen.getByRole("status")).toHaveTextContent("conference-2");
  await waitFor(() =>
    expect(api.documentVersions).toHaveBeenLastCalledWith("ws", "conference"),
  );
});
it("keeps document identity while reading an old version and selecting it as current", async () => {
  render(<Harness />);
  await screen.findByRole("option", { name: /Nov 1, 2026/ });
  fireEvent.change(screen.getByLabelText("Document version"), {
    target: { value: "seminar-1" },
  });
  await screen.findByText("Viewing an earlier version.");
  expect(screen.getByLabelText("Document")).toHaveValue("seminar-2");
  fireEvent.click(screen.getByRole("button", { name: "Make current version" }));
  await waitFor(() =>
    expect(api.updateDocument).toHaveBeenCalledWith(
      expect.objectContaining({
        paperId: "seminar",
        currentRevisionId: "seminar-1",
        documentType: "slides",
      }),
    ),
  );
  expect(api.updateDocument.mock.calls[0][0].paperId).not.toBe("paper");
});
it("edits title and document type without creating a new document or revision", async () => {
  render(<Harness />);
  fireEvent.click(screen.getByText("Slides · Document details"));
  fireEvent.change(screen.getByLabelText("Document name"), {
    target: { value: "Response to referees" },
  });
  fireEvent.change(screen.getByLabelText("Document type"), {
    target: { value: "response" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save details" }));
  await waitFor(() =>
    expect(api.updateDocument).toHaveBeenCalledWith(
      expect.objectContaining({
        paperId: "seminar",
        title: "Response to referees",
        documentType: "response",
        expectedUpdatedAt: docs[1].paper.updatedAt,
      }),
    ),
  );
});
