import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import ProjectDocuments from "./ProjectDocuments";
import type { SurfaceApi } from "./shared";
import type { PaperWithRevision } from "../../lib/workbenchTypes";
import { makeProjectHome } from "../../test/projectSurfaceFixtures";

vi.mock("../WorkspaceContextTray", () => ({ addContextObject: vi.fn() }));
vi.mock("../../lib/projectClient", () => ({ projectClient: {} }));
vi.mock("../WorkspaceDocumentReader", () => ({
  default: ({
    revisionId,
    readonly,
  }: {
    revisionId: string;
    readonly?: boolean;
  }) => (
    <div data-testid="reader">
      {revisionId}
      {readonly ? " (pinned)" : ""}
    </div>
  ),
}));

const paper = (id: string, title: string) =>
  ({
    paper: { id: `paper-${id}`, title },
    revision: { id, capturedAt: "2026-10-01T00:00:00Z" },
  }) as PaperWithRevision;

function Harness() {
  const [revisionId, setRevisionId] = useState<string | null>("rev-a");
  const [pinnedRevision, setPinnedRevision] = useState<string | null>(null);
  const data = {
    ...makeProjectHome(),
    papers: [paper("rev-a", "Draft A"), paper("rev-b", "Draft B")],
  };
  return (
    <ProjectDocuments
      {...({
        workspaceId: "workspace",
        data,
        saveSettings: vi.fn(),
        reportError: vi.fn(),
        openAnnotation: vi.fn(),
      } as unknown as SurfaceApi)}
      revisionId={revisionId}
      setRevisionId={setRevisionId}
      pinnedRevision={pinnedRevision}
      setPinnedRevision={setPinnedRevision}
      initialSelection={undefined}
      setInitialSelection={vi.fn()}
      pinnedSelection={undefined}
      selectionAction={null}
      selectionBody=""
      setSelectionBody={vi.fn()}
      onSelection={vi.fn()}
      onSubmitSelection={vi.fn()}
      onCancelSelection={vi.fn()}
      onImport={vi.fn()}
      onAddVersion={vi.fn()}
    />
  );
}

it("replaces the primary reader when another document is chosen during a comparison", async () => {
  render(<Harness />);
  await screen.findByText("rev-a");
  fireEvent.click(screen.getByRole("button", { name: "Compare with another" }));
  await waitFor(() => expect(screen.getAllByTestId("reader")).toHaveLength(2));
  fireEvent.change(screen.getByRole("combobox", { name: "Document" }), {
    target: { value: "rev-b" },
  });
  await waitFor(() =>
    expect(
      screen.getAllByTestId("reader").map((reader) => reader.textContent),
    ).toEqual(["rev-b", "rev-a (pinned)"]),
  );
});
