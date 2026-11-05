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

it("reveals and focuses the optional note, retains its collapsed excerpt, and removes it", async () => {
  const user = userEvent.setup();
  render(<Editor />);
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  await user.click(
    screen.getByRole("button", { name: /Additional instructions/ }),
  );
  const input = screen.getByRole("textbox");
  expect(input).toHaveFocus();
  await user.type(input, "Prioritize the model.");
  await user.click(
    screen.getByRole("button", { name: /Additional instructions/ }),
  );
  expect(
    screen.getByText("Prioritize the model.", { selector: "p" }),
  ).toBeVisible();
  await user.click(
    screen.getByRole("button", { name: /Additional instructions/ }),
  );
  expect(input).toHaveValue("Prioritize the model.");
  await user.click(screen.getByRole("button", { name: "Clear" }));
  expect(input).toHaveValue("");
  expect(input).toHaveFocus();
  expect(screen.getByText(/Optional ·/)).toBeVisible();
});

it("counts Unicode characters and retains over-limit text with an accessible error", async () => {
  const user = userEvent.setup();
  render(<Editor />);
  await user.click(
    screen.getByRole("button", { name: /Additional instructions/ }),
  );
  const input = screen.getByRole("textbox");
  fireEvent.change(input, { target: { value: "🦀".repeat(4000) } });
  expect(input).toHaveAttribute("aria-invalid", "false");
  fireEvent.change(input, { target: { value: "🦀".repeat(4001) } });
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(screen.getByRole("alert")).toHaveTextContent("4,000");
  expect(input).toHaveValue("🦀".repeat(4001));
});

it("normalizes only outer Unicode whitespace and line endings, preserving literal placeholders", () => {
  expect(normalizeAuthorNote(" \u0085\r\n\t")).toBeNull();
  expect(normalizeAuthorNote("  Model\r\n\r\n{var:key}  ")).toBe(
    "Model\n\n{var:key}",
  );
  expect(authorNoteLength(" 🦀 ")).toBe(1);
});
