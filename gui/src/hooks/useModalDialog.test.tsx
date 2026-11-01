import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import useModalDialog from "./useModalDialog";

function Dialog({ onClose }: { onClose: () => void }) {
  const ref = useModalDialog<HTMLDivElement>(onClose);
  return (
    <div ref={ref} role="dialog" aria-modal="true" aria-label="Search">
      <input aria-label="Query" />
    </div>
  );
}

it("leaves an Escape that only cancels an input-method composition", () => {
  const onClose = vi.fn();
  render(<Dialog onClose={onClose} />);
  const field = screen.getByRole("textbox", { name: "Query" });
  fireEvent.keyDown(field, { key: "Escape", isComposing: true });
  fireEvent.keyDown(field, { key: "Escape", keyCode: 229 });
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(onClose).toHaveBeenCalledOnce();
});
