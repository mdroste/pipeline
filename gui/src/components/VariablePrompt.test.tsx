import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import VariablePrompt from "./VariablePrompt";

const open = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open options
      </button>
      {open && (
        <VariablePrompt
          variables={[{ key: "audience", label: "Audience", kind: "text" }]}
          inputSlots={[{ key: "rubric", label: "Rubric", required: true }]}
          onCancel={() => setOpen(false)}
          onSubmit={() => setOpen(false)}
        />
      )}
    </>
  );
}

describe("VariablePrompt accessibility", () => {
  beforeEach(() => {
    open.mockReset();
  });

  it("labels fields, traps focus, closes on Escape, and restores the opener", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Open options" });
    await user.click(opener);

    const dialog = screen.getByRole("dialog", { name: "Report options" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    const audience = screen.getByRole("textbox", { name: "Audience" });
    expect(screen.getByRole("textbox", { name: /Rubric/ })).toHaveAttribute(
      "aria-required",
      "true",
    );
    await waitFor(() => expect(audience).toHaveFocus());

    const cancel = screen.getByRole("button", { name: "Cancel" });
    cancel.focus();
    await user.tab();
    expect(audience).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("dialog", { name: "Report options" }),
    ).not.toBeInTheDocument();
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("offers only backend-supported formats for a document input slot", async () => {
    open.mockResolvedValue("/inputs/rubric.pdf");
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Open options" }));
    await user.click(screen.getByRole("button", { name: "Browse…" }));

    expect(open).toHaveBeenCalledWith({
      multiple: false,
      directory: false,
      filters: [
        {
          name: "Rubric",
          extensions: ["pdf", "tex", "docx"],
        },
      ],
    });
  });

  it("surfaces a named-input picker failure instead of treating it as cancellation", async () => {
    open.mockRejectedValueOnce(new Error("dialog permission denied"));
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "Open options" }));
    await user.click(screen.getByRole("button", { name: "Browse…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker for “Rubric”: dialog permission denied",
    );
  });

  it("surfaces a file-variable picker failure with the variable name", async () => {
    open.mockRejectedValueOnce(new Error("dialog unavailable"));
    const user = userEvent.setup();
    render(
      <VariablePrompt
        variables={[{ key: "source", label: "Source data", kind: "file" }]}
        onCancel={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Browse…" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not open the file picker for “Source data”: dialog unavailable",
    );
  });
});
