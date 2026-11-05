import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import {
  ConditionEditor,
  modelFromCondition,
  modelToCondition,
} from "./ConditionEditor";
import type { Chain, Condition } from "../../lib/taskTypes";

const chain: Chain = {
  schemaVersion: 1,
  name: "Conditions",
  description: "",
  steps: [],
  limits: { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 },
};
const left = { kind: "input", key: "value" } as const;

function Harness({
  initial,
  onChange,
}: {
  initial: Condition;
  onChange: (condition: Condition) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <ConditionEditor
      chain={chain}
      targetStepId={null}
      value={value}
      onChange={(next) => {
        onChange(next);
        setValue(next);
      }}
      label="Only if"
    />
  );
}

const field = () =>
  screen.getByRole("textbox", { name: "Only if comparison value 1" });

it("keeps typed equality text intact and commits it as a plain string", () => {
  const onChange = vi.fn();
  render(
    <Harness initial={{ op: "equals", left, right: "" }} onChange={onChange} />,
  );
  for (const text of ["a", "ap", "approved"])
    fireEvent.change(field(), { target: { value: text } });
  expect(onChange).not.toHaveBeenCalled();
  expect(field()).toHaveValue("approved");
  fireEvent.blur(field());
  expect(onChange).toHaveBeenLastCalledWith({
    op: "equals",
    left,
    right: "approved",
  });
  expect(field()).toHaveValue("approved");
});

it("reads number literals in equality text and quotes strings that look like them", () => {
  const onChange = vi.fn();
  render(
    <Harness
      initial={{ op: "equals", left, right: "42" }}
      onChange={onChange}
    />,
  );
  expect(field()).toHaveValue('"42"');
  fireEvent.change(field(), { target: { value: "42" } });
  fireEvent.keyDown(field(), { key: "Enter" });
  expect(onChange).toHaveBeenLastCalledWith({ op: "equals", left, right: 42 });
  expect(field()).toHaveValue("42");
});

it("accepts decimal and negative thresholds and reverts unreadable ones", () => {
  const onChange = vi.fn();
  render(
    <Harness
      initial={{ op: "lessThan", left, right: 1 }}
      onChange={onChange}
    />,
  );
  for (const text of ["0", "0.", "0.5"])
    fireEvent.change(field(), { target: { value: text } });
  expect(field()).toHaveValue("0.5");
  fireEvent.blur(field());
  expect(onChange).toHaveBeenLastCalledWith({
    op: "lessThan",
    left,
    right: 0.5,
  });

  fireEvent.change(field(), { target: { value: "-" } });
  fireEvent.change(field(), { target: { value: "-2" } });
  fireEvent.blur(field());
  expect(onChange).toHaveBeenLastCalledWith({
    op: "lessThan",
    left,
    right: -2,
  });

  onChange.mockClear();
  fireEvent.change(field(), { target: { value: "abc" } });
  fireEvent.blur(field());
  expect(onChange).not.toHaveBeenCalled();
  expect(field()).toHaveValue("-2");
});

it("round-trips equality values of every JSON type", () => {
  for (const right of [
    "approved",
    "42",
    "true",
    "",
    3.5,
    false,
    null,
    {
      a: 1,
    },
  ]) {
    const condition: Condition = { op: "equals", left, right };
    expect(modelToCondition(modelFromCondition(condition)!)).toEqual(condition);
  }
});
