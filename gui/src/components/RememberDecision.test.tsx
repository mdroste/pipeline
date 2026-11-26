import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import RememberDecision from "./RememberDecision";
const createNote = vi.hoisted(() => vi.fn().mockResolvedValue({}));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { createNote },
}));
it("saves an explicitly edited decision as a proposal with transcript provenance", async () => {
  render(
    <RememberDecision
      workspaceId="p"
      conversationId="c"
      messageId="m"
      text="Long response"
    />,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "Remember this decision" }),
  );
  expect(createNote).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("Decision to remember"), {
    target: { value: "Use the balanced sample" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save proposed note" }));
  await waitFor(() =>
    expect(createNote).toHaveBeenCalledWith(
      expect.objectContaining({
        workspaceId: "p",
        state: "proposed",
        pinned: false,
        body: "Use the balanced sample\n\nSource: conversation c, message m.",
      }),
    ),
  );
});
