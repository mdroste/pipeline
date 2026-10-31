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
