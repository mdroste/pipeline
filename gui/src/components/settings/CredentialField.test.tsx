import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import CredentialField from "./CredentialField";

it("keeps the key and its save action on one line", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<CredentialField label="Test Key" value="" onSave={onSave} />);

  const save = screen.getByRole("button", { name: "Save Test Key" });
  expect(save).toHaveTextContent(/^Save$/);
  expect(screen.getByLabelText("Test Key").parentElement).toBe(
    save.parentElement,
  );
  expect(
    screen.queryByRole("button", { name: "Cancel" }),
  ).not.toBeInTheDocument();

  await user.type(screen.getByLabelText("Test Key"), " secret ");
  expect(screen.getByRole("button", { name: "Cancel" })).toBeVisible();
  await user.click(save);
  expect(onSave).toHaveBeenCalledWith("secret");
});

it("offers Edit for a saved key and can back out of the change", async () => {
  const user = userEvent.setup();
  render(<CredentialField label="Test Key" value="saved" onSave={vi.fn()} />);

  await user.click(screen.getByRole("button", { name: "Edit Test Key" }));
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.getByText("API key saved · encrypted")).toBeVisible();
});

// What redact_settings sends for a saved key.
const STORED = "__PIPELINE_STORED_SECRET__";

it("keeps the stored-key marker out of a replacement key", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<CredentialField label="Test Key" value={STORED} onSave={onSave} />);

  await user.click(screen.getByRole("button", { name: "Edit Test Key" }));
  const field = screen.getByLabelText("Test Key");
  expect(field).toHaveValue("");
  expect(field).toHaveAttribute("placeholder", "Enter a new key");
  expect(screen.getByRole("button", { name: "Save Test Key" })).toBeDisabled();

  await user.click(field);
  await user.paste("sk-new-key");
  await user.click(screen.getByRole("button", { name: "Save Test Key" }));
  expect(onSave).toHaveBeenCalledWith("sk-new-key");
});

it("removes a saved key only when asked", async () => {
  const user = userEvent.setup();
  const onSave = vi.fn();
  render(<CredentialField label="Test Key" value={STORED} onSave={onSave} />);

  await user.click(screen.getByRole("button", { name: "Edit Test Key" }));
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  expect(onSave).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: "Edit Test Key" }));
  await user.click(screen.getByRole("button", { name: "Remove Test Key" }));
  expect(onSave).toHaveBeenCalledWith("");
});
