import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import AuthorNoteField from "./AuthorNoteField";
import { authorNoteLength, normalizeAuthorNote } from "../lib/authorNote";

function Editor() {
  const [value, setValue] = useState("");
  return <AuthorNoteField value={value} onChange={setValue} />;
}

it("keeps instructions visible and restores focus after clearing", async () => {
  const user = userEvent.setup();
  render(<Editor />);
  const input = screen.getByRole("textbox", {
    name: "Additional instructions",
  });
  expect(input).toBeVisible();
  await user.type(input, "Prioritize the model.");
  expect(input).toHaveValue("Prioritize the model.");
  await user.click(screen.getByRole("button", { name: "Clear" }));
  expect(input).toHaveValue("");
  expect(input).toHaveFocus();
  expect(screen.getByText("Optional")).toBeVisible();
  expect(
    screen.queryByText("Shared with every review agent."),
  ).not.toBeInTheDocument();
});

it("counts Unicode characters and retains over-limit text with an accessible error", async () => {
  render(<Editor />);
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "🦀".repeat(4000) } });
  expect(input).toHaveAttribute("aria-invalid", "false");
  fireEvent.change(input, { target: { value: "🦀".repeat(4001) } });
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("alert")).toHaveTextContent("4,000");
  expect(input).toHaveValue("🦀".repeat(4001));
});

it("focuses instructions when requested by the launch flow without clearing the draft", () => {
  const { rerender } = render(
    <AuthorNoteField value="Keep this note" onChange={() => {}} />,
  );
  rerender(
    <AuthorNoteField value="Keep this note" onChange={() => {}} focusKey={1} />,
  );
  expect(screen.getByRole("textbox")).toHaveFocus();
  expect(screen.getByRole("textbox")).toHaveValue("Keep this note");
});

it("normalizes only outer Unicode whitespace and line endings, preserving literal placeholders", () => {
  expect(normalizeAuthorNote(" \u0085\r\n\t")).toBeNull();
  expect(normalizeAuthorNote("  Model\r\n\r\n{var:key}  ")).toBe(
    "Model\n\n{var:key}",
  );
  expect(authorNoteLength(" 🦀 ")).toBe(1);
});
