import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import AddStepDialog from "./AddStepDialog";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));

describe("AddStepDialog backdrop", () => {
  it("closes on a press that starts on the backdrop", () => {
    const onCancel = vi.fn();
    render(<AddStepDialog onCreate={vi.fn()} onCancel={onCancel} />);
    const overlay = screen.getByRole("dialog", { name: "Add workflow step" })
      .parentElement as HTMLElement;

    fireEvent.mouseDown(overlay);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("keeps the draft when a drag started inside the dialog releases over the backdrop", () => {
    const onCancel = vi.fn();
    render(<AddStepDialog onCreate={vi.fn()} onCancel={onCancel} />);
    const overlay = screen.getByRole("dialog", { name: "Add workflow step" })
      .parentElement as HTMLElement;
    const nameField = screen.getByRole("textbox", { name: "Step name" });

    // A text-selection drag: press inside a field, release over the backdrop.
    // The browser then dispatches click on the overlay (their common ancestor).
    fireEvent.mouseDown(nameField);
    fireEvent.mouseUp(overlay);
    fireEvent.click(overlay);
    expect(onCancel).not.toHaveBeenCalled();
  });
});
