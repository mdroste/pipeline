import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, expect, it, vi } from "vitest";
import { NumberField } from "./NumberField";
import { Timing } from "./Timing";
import type { Trigger } from "../../lib/taskTypes";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.invoke.mockResolvedValue([]);
});

function Field({ onChange }: { onChange: (value: number) => void }) {
  const [value, setValue] = useState(3);
  return (
    <NumberField
      aria-label="Rounds"
      min={1}
      max={32}
      value={value}
      onChange={(next) => {
        onChange(next);
        setValue(next);
      }}
    />
  );
}

it("keeps a cleared field editable and commits the typed number on blur", () => {
  const onChange = vi.fn();
  render(<Field onChange={onChange} />);
  const input = screen.getByRole("spinbutton", { name: "Rounds" });
  fireEvent.change(input, { target: { value: "" } });
  expect(input).toHaveValue(null);
  fireEvent.change(input, { target: { value: "1" } });
  fireEvent.change(input, { target: { value: "15" } });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.blur(input);
  expect(onChange).toHaveBeenCalledWith(15);
  expect(input).toHaveValue(15);
});

it("clamps out-of-range input and reverts empty input", () => {
  const onChange = vi.fn();
  render(<Field onChange={onChange} />);
  const input = screen.getByRole("spinbutton", { name: "Rounds" });
  fireEvent.change(input, { target: { value: "99" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(onChange).toHaveBeenLastCalledWith(32);
  fireEvent.change(input, { target: { value: "" } });
  fireEvent.blur(input);
  expect(onChange).toHaveBeenCalledTimes(1);
  expect(input).toHaveValue(32);
  fireEvent.change(input, { target: { value: "0" } });
  fireEvent.blur(input);
  expect(onChange).toHaveBeenLastCalledWith(1);
});

it("lets the interval be retyped instead of snapping a cleared field to 1", () => {
  const onChange = vi.fn();
  const trigger: Trigger = { kind: "interval", seconds: 3600, anchor: 100 };
  render(<Timing value={trigger} onChange={onChange} />);
  const input = screen.getByRole("spinbutton", {
    name: "Interval in minutes",
  });
  fireEvent.change(input, { target: { value: "" } });
  fireEvent.change(input, { target: { value: "15" } });
  expect(onChange).not.toHaveBeenCalled();
  fireEvent.blur(input);
  expect(onChange).toHaveBeenCalledWith({ ...trigger, seconds: 900 });
});
