import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import ImportDocumentDialog from "./ImportDocumentDialog";
const api = vi.hoisted(() => ({ importPaper: vi.fn() }));
vi.mock("../../lib/workbench/researchClient", () => ({ researchClient: api }));
beforeEach(() => {
  vi.clearAllMocks();
  api.importPaper.mockResolvedValue({
    paper: { id: "deck" },
    revision: { id: "v1" },
  });
});
it("imports a named slide deck with an explicit type and retries refresh without duplicating it", async () => {
  const imported = vi
    .fn()
    .mockRejectedValueOnce(new Error("Refresh failed"))
    .mockResolvedValue(undefined);
  const close = vi.fn();
  render(
    <ImportDocumentDialog
      workspaceId="ws"
      path="/research/talk.pdf"
      folder={false}
      onClose={close}
      onImported={imported}
    />,
  );
  fireEvent.change(screen.getByLabelText("Name"), {
    target: { value: "Seminar presentation" },
  });
  fireEvent.change(screen.getByLabelText("Document type"), {
    target: { value: "slides" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add document" }));
  await screen.findByRole("alert");
  expect(api.importPaper).toHaveBeenCalledWith(
    expect.objectContaining({
      title: "Seminar presentation",
      documentType: "slides",
      role: "other",
      paperId: null,
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Add document" }));
  await waitFor(() => expect(close).toHaveBeenCalled());
  expect(api.importPaper).toHaveBeenCalledTimes(1);
});
