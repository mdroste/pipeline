import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import MemoryTab from "./MemoryTab";
import type { ResearchNote } from "../../lib/workbench/researchTypes";
import { router } from "../../lib/router";

afterEach(cleanup);

it("focuses a linked note when it arrives after navigation", async () => {
  router.init({
    page: "workspace",
    projectId: "w",
    destination: "memory",
    noteId: "n",
  });
  const props = {
    busy: false,
    act: vi.fn(),
    noteKind: "question" as const,
    setNoteKind: vi.fn(),
    noteBody: "",
    setNoteBody: vi.fn(),
    workspaceId: "w",
    snapshot: null,
  };
  const view = render(<MemoryTab {...props} notes={[]} />);
  view.rerender(
    <MemoryTab
      {...props}
      notes={[
        {
          id: "n",
          kind: "question",
          state: "accepted",
          origin: "user",
          body: "Linked assumption",
          revision: 1,
        } as ResearchNote,
      ]}
    />,
  );
  await waitFor(() =>
    expect(document.querySelector('[data-note-id="n"]')).toHaveFocus(),
  );
  expect(document.querySelector('[data-note-id="n"]')).toHaveAttribute(
    "aria-current",
    "location",
  );
  router.init({ page: "home" });
});

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
