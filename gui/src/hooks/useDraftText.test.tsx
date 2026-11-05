import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import useDraftText from "./useDraftText";

function ChoiceList({ onCommit }: { onCommit?: (choices: string[]) => void }) {
  const [choices, setChoices] = useState(["low"]);
  const field = useDraftText(choices.join(", "), (text) => {
    const next = text
      .split(",")
      .map((choice) => choice.trim())
      .filter(Boolean);
    setChoices(next);
    onCommit?.(next);
  });
  return (
    <>
      <input aria-label="Choices" {...field} />
      <output>{choices.join("|")}</output>
      <button onClick={() => setChoices(["outside"])}>Replace</button>
    </>
  );
}

it("keeps separators and spaces while typing and commits on blur", () => {
  const onCommit = vi.fn();
  render(<ChoiceList onCommit={onCommit} />);
  const input = screen.getByLabelText("Choices");
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: "low, " } });
  expect(input).toHaveValue("low, ");
  fireEvent.change(input, { target: { value: "low, very high" } });
  expect(input).toHaveValue("low, very high");
  expect(onCommit).not.toHaveBeenCalled();
  fireEvent.blur(input);
  expect(onCommit).toHaveBeenCalledWith(["low", "very high"]);
  expect(screen.getByRole("status")).toHaveTextContent("low|very high");
  expect(input).toHaveValue("low, very high");
});

it("normalizes the draft after a commit that does not change the value", () => {
  render(<ChoiceList />);
  const input = screen.getByLabelText("Choices");
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: "low, " } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(input).toHaveValue("low");
});

it("commits the draft on Cmd/Ctrl+S before a window save listener runs", () => {
  const seen: string[] = [];
  const onWindowKeyDown = (event: KeyboardEvent) => {
    if (event.key.toLowerCase() === "s") {
      seen.push(
        `${event.defaultPrevented}:${screen.getByRole("status").textContent}`,
      );
    }
  };
  window.addEventListener("keydown", onWindowKeyDown);
  try {
    const onCommit = vi.fn();
    render(<ChoiceList onCommit={onCommit} />);
    const input = screen.getByLabelText("Choices");
    fireEvent.focus(input);
    fireEvent.change(input, { target: { value: "low, high" } });
    fireEvent.keyDown(input, { key: "s", metaKey: true });
    fireEvent.change(input, { target: { value: "low, high, top" } });
    fireEvent.keyDown(input, { key: "S", ctrlKey: true, shiftKey: true });
    expect(onCommit).toHaveBeenNthCalledWith(1, ["low", "high"]);
    expect(onCommit).toHaveBeenNthCalledWith(2, ["low", "high", "top"]);
    expect(seen).toEqual(["false:low|high", "false:low|high|top"]);
  } finally {
    window.removeEventListener("keydown", onWindowKeyDown);
  }
});

it("restores the canonical text on Escape and follows outside changes when idle", () => {
  render(<ChoiceList />);
  const input = screen.getByLabelText("Choices");
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: "draft" } });
  fireEvent.keyDown(input, { key: "Escape" });
  expect(input).toHaveValue("low");
  fireEvent.click(screen.getByText("Replace"));
  expect(input).toHaveValue("outside");
});
