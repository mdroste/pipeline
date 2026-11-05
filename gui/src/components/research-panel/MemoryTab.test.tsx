import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import MemoryTab from "./MemoryTab";
import type { ResearchNote } from "../../lib/workbenchTypes";

afterEach(cleanup);

it("names its fields and shows stored note values in plain words", () => {
  render(
    <MemoryTab
      busy={false}
      act={vi.fn()}
      noteKind="next_step"
      setNoteKind={vi.fn()}
      noteBody=""
      setNoteBody={vi.fn()}
      notes={[
        {
          id: "n",
          kind: "next_step",
          state: "proposed",
          origin: "user",
          body: "Rerun the model",
          pinned: false,
          revision: 1,
        } as ResearchNote,
      ]}
      workspaceId="w"
      snapshot={null}
    />,
  );
  expect(screen.getByLabelText("Note type")).toHaveDisplayValue("Next step");
  expect(screen.getByLabelText("Note text")).toBeInTheDocument();
  expect(screen.getByText("· Proposed")).toBeInTheDocument();
  expect(screen.getByText("· You")).toBeInTheDocument();
  expect(document.body.textContent).not.toMatch(/next_step|proposed|user/);
});
