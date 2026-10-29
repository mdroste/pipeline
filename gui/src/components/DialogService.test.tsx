import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, expect, it } from "vitest";
import DialogService, { confirmDialog, promptDialog } from "./DialogService";

// jsdom has no modal dialog support; mirror the open attribute it toggles.
const original = {
  showModal: HTMLDialogElement.prototype.showModal,
  close: HTMLDialogElement.prototype.close,
  open: Object.getOwnPropertyDescriptor(HTMLDialogElement.prototype, "open"),
};
beforeAll(() => {
  Object.defineProperty(HTMLDialogElement.prototype, "open", {
    configurable: true,
    get(this: HTMLDialogElement) {
      return this.hasAttribute("open");
    },
  });
  HTMLDialogElement.prototype.showModal = function (this: HTMLDialogElement) {
    this.setAttribute("open", "");
  };
  HTMLDialogElement.prototype.close = function (this: HTMLDialogElement) {
    this.removeAttribute("open");
  };
});
afterAll(() => {
  HTMLDialogElement.prototype.showModal = original.showModal;
  HTMLDialogElement.prototype.close = original.close;
  if (original.open)
    Object.defineProperty(HTMLDialogElement.prototype, "open", original.open);
  else delete (HTMLDialogElement.prototype as { open?: boolean }).open;
});

function mount() {
  return render(
    <DialogService>
      <button type="button">Origin</button>
    </DialogService>,
  );
}

it("resolves a prompt with the edited text and restores focus", async () => {
  const user = userEvent.setup();
  mount();
  const origin = screen.getByRole("button", { name: "Origin" });
  origin.focus();
  let answer: Promise<string | null> = Promise.resolve(null);
  act(() => {
    answer = promptDialog("Name this conversation.", {
      title: "Rename conversation",
      label: "Conversation title",
      defaultValue: "Old title",
      confirmLabel: "Rename",
    });
  });
  const field = await screen.findByRole("textbox", {
    name: "Conversation title",
  });
  expect(field).toHaveValue("Old title");
  expect(field).toHaveFocus();
  await user.clear(field);
  await user.type(field, "New title{Enter}");
  await expect(answer).resolves.toBe("New title");
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  expect(origin).toHaveFocus();
});

it("resolves a cancelled prompt with null, including Escape", async () => {
  const user = userEvent.setup();
  mount();
  let first: Promise<string | null> = Promise.resolve("");
  act(() => {
    first = promptDialog("Name", { defaultValue: "Kept" });
  });
  await user.click(await screen.findByRole("button", { name: "Cancel" }));
  await expect(first).resolves.toBeNull();

  let second: Promise<string | null> = Promise.resolve("");
  act(() => {
    second = promptDialog("Name", { defaultValue: "Kept" });
  });
  const dialog = document.querySelector("dialog")!;
  fireEvent(dialog, new Event("cancel", { cancelable: true }));
  await expect(second).resolves.toBeNull();
});

it("queues a second request instead of stranding the first", async () => {
  const user = userEvent.setup();
  mount();
  let first: Promise<boolean> = Promise.resolve(false);
  let second: Promise<boolean> = Promise.resolve(false);
  act(() => {
    first = confirmDialog("Leave this page?", { confirmLabel: "Leave" });
    second = confirmDialog("Quit Pipeline?", { confirmLabel: "Quit" });
  });
  expect(await screen.findByText("Leave this page?")).toBeVisible();
  expect(screen.queryByText("Quit Pipeline?")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Leave" }));
  await expect(first).resolves.toBe(true);
  expect(await screen.findByText("Quit Pipeline?")).toBeVisible();
  expect(document.querySelector("dialog")).toHaveAttribute("open");
  await user.click(screen.getByRole("button", { name: "Cancel" }));
  await expect(second).resolves.toBe(false);
  expect(document.querySelector("dialog")).not.toHaveAttribute("open");
});

it("resolves to cancel when no dialog host is mounted", async () => {
  await expect(confirmDialog("Delete?")).resolves.toBe(false);
  await expect(promptDialog("Name?")).resolves.toBeNull();
});
