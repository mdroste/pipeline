import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
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

  it("offers every format a run accepts for a slot with no allowed types", async () => {
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
          extensions: ["pdf", "tex", "docx", "md", "markdown", "txt"],
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

describe("VariablePrompt layout", () => {
  it("scrolls a long form between a fixed title and buttons", () => {
    render(
      <VariablePrompt
        variables={Array.from({ length: 12 }, (_, index) => ({
          key: `value${index}`,
          label: `Value ${index}`,
          kind: "text",
        }))}
        inputSlots={[]}
        onCancel={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog", { name: "Report options" });
    expect(dialog).toHaveClass("max-h-[90vh]", "flex", "flex-col");
    const form = screen.getByLabelText("Value 0").closest(".overflow-y-auto");
    expect(form).toHaveClass("min-h-0", "flex-1");
    const buttons = screen.getByRole("button", { name: "Continue" });
    expect(form).not.toContainElement(buttons);
    expect(form).not.toContainElement(
      screen.getByRole("heading", { name: "Report options" }),
    );
  });
});

describe("VariablePrompt values", () => {
  it("starts a choice without a default unselected, so what is shown is what is sent", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();
    render(
      <VariablePrompt
        variables={[
          {
            key: "approach",
            label: "Approach",
            kind: "choice",
            choices: ["empirical", "theory"],
            default: "",
            required: true,
          },
          {
            key: "venue",
            label: "Venue",
            kind: "choice",
            choices: ["journal", "conference"],
          },
        ]}
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const approach = screen.getByRole("combobox", { name: /Approach/ });
    expect(approach).toHaveValue("");
    expect(approach).toHaveDisplayValue("Choose…");
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();

    await user.selectOptions(approach, "theory");
    expect(
      within(approach).queryByRole("option", { name: "Choose…" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(onSubmit).toHaveBeenCalledWith(
      { approach: "theory", venue: "" },
      {},
    );
  });

  it("checks length and format rules before Continue", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();
    render(
      <VariablePrompt
        variables={[
          {
            key: "code",
            label: "Code",
            validation: { min_length: 3, max_length: 5, pattern: "^[A-Z]+$" },
          },
          {
            key: "note",
            label: "Note",
            // Not readable by this browser; the backend decides.
            validation: { pattern: "(?i)note" },
          },
        ]}
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const code = screen.getByRole("textbox", { name: "Code" });
    const note = screen.getByRole("textbox", { name: "Note" });
    const proceed = screen.getByRole("button", { name: "Continue" });

    await user.type(code, "AB");
    expect(code).toHaveAccessibleDescription("Enter at least 3 characters.");
    expect(code).toHaveAttribute("aria-invalid", "true");
    expect(proceed).toBeDisabled();

    await user.type(code, "CDEF");
    expect(code).toHaveAccessibleDescription(
      "Enter no more than 5 characters.",
    );
    await user.clear(code);
    await user.type(code, "abc");
    expect(code).toHaveAccessibleDescription(
      "This value does not match the format the review requires.",
    );
    expect(proceed).toBeDisabled();

    await user.clear(code);
    await user.type(code, "ABC");
    await user.type(note, "anything");
    expect(code).not.toHaveAttribute("aria-invalid");
    expect(proceed).toBeEnabled();
    await user.click(proceed);
    expect(onSubmit).toHaveBeenCalledWith(
      { code: "ABC", note: "anything" },
      {},
    );
  });

  it("reopens with the submitted values and the launch error", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();
    render(
      <VariablePrompt
        variables={[
          { key: "code", label: "Code", default: "AAA" },
          { key: "token", label: "Token", secret: true },
        ]}
        inputSlots={[{ key: "rubric", label: "Rubric", required: true }]}
        initialValues={{ code: "XYZ", token: "s3cret" }}
        initialInputs={{ rubric: "/inputs/rubric.pdf" }}
        error="The workflow execution plan could not be prepared: code is invalid"
        onCancel={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent("code is invalid");
    expect(screen.getByRole("textbox", { name: "Code" })).toHaveValue("XYZ");
    expect(screen.getByLabelText("Token")).toHaveValue("s3cret");
    expect(screen.getByRole("textbox", { name: /Rubric/ })).toHaveValue(
      "/inputs/rubric.pdf",
    );
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(onSubmit).toHaveBeenCalledWith(
      { code: "XYZ", token: "s3cret" },
      { rubric: "/inputs/rubric.pdf" },
    );
  });
});
