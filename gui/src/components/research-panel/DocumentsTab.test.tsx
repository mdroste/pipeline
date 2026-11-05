import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import DocumentsTab from "./DocumentsTab";
import type { PaperWithRevision, SourceRecord } from "../../lib/workbenchTypes";

afterEach(cleanup);

it("names the search box and shows stored kinds in plain words", () => {
  const paper = {
    paper: { id: "p", title: "Draft", role: "manuscript" },
    revision: {
      id: "r",
      inputKind: "source_tree",
      contentHash: "abc",
      extraction: { status: "complete" },
    },
  } as unknown as PaperWithRevision;
  render(
    <DocumentsTab
      busy={false}
      act={vi.fn()}
      papers={[paper]}
      sources={[
        {
          id: "s",
          title: "Card (1994)",
          accessState: "partial",
          acquiredVia: "local_pdf",
          contentHash: "abc",
        } as SourceRecord,
      ]}
      selectedPaper={paper}
      setProjectPaperId={vi.fn()}
      patchSession={vi.fn()}
      query=""
      setQuery={vi.fn()}
      hits={[]}
      setHits={vi.fn()}
      workspaceId="w"
    />,
  );
  expect(screen.getByLabelText("Search this version")).toBeInTheDocument();
  expect(screen.getByText(/Folder of source files/)).toBeInTheDocument();
  expect(screen.getByText(/Part of the text · Local PDF/)).toBeInTheDocument();
  expect(document.body.textContent).not.toMatch(/source_tree|local_pdf/);
});
