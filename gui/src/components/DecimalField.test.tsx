import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, it, vi } from "vitest";
import DecimalField from "./DecimalField";

function Harness({
  initial = 0.5,
  onChange,
}: {
  initial?: number;
  onChange: (value: number) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <DecimalField
      aria-label="Offset"
      value={value}
      onChange={(next) => {
        setValue(next);
        onChange(next);
      }}
    />
  );
}
const field = () => screen.getByLabelText("Offset") as HTMLInputElement;
async function replaceWith(text: string) {
  const user = userEvent.setup();
  await user.clear(field());
  if (text) await user.type(field(), text);
  return user;
}

it.each(["-0.25", "-1", "0.25", "1e-3", "-.5"])(
  "keeps %s while typing and saves it on blur",
  async (text) => {
    const changed = vi.fn();
    render(<Harness onChange={changed} />);
    const user = await replaceWith(text);
    expect(changed).not.toHaveBeenCalled();
    await user.tab();
    expect(changed).toHaveBeenCalledExactlyOnceWith(Number(text));
    expect(Number(field().value)).toBe(Number(text));
  },
);

it("saves on Enter", async () => {
  const changed = vi.fn();
  render(<Harness onChange={changed} />);
  const user = await replaceWith("-2");
  await user.keyboard("{Enter}");
  expect(changed).toHaveBeenCalledExactlyOnceWith(-2);
});

it("reverts an empty field rather than saving zero", async () => {
  const changed = vi.fn();
  render(<Harness onChange={changed} />);
  const user = await replaceWith("");
  await user.tab();
  expect(changed).not.toHaveBeenCalled();
  expect(field()).toHaveValue(0.5);
});

it("reverts a lone minus sign rather than saving zero", async () => {
  const changed = vi.fn();
  render(<Harness onChange={changed} />);
  const user = await replaceWith("-");
  await user.tab();
  expect(changed).not.toHaveBeenCalled();
  expect(field()).toHaveValue(0.5);
});

it("never commits while the browser reports unreadable input", () => {
  const changed = vi.fn();
  render(<Harness onChange={changed} />);
  const input = field();
  fireEvent.focus(input);
  fireEvent.change(input, { target: { value: "7" } });
  // Browsers vary in what `value` holds here, so the flag decides.
  Object.defineProperty(input, "validity", { value: { badInput: true } });
  fireEvent.blur(input);
  expect(changed).not.toHaveBeenCalled();
});

it("restores the saved value on Escape", async () => {
  const changed = vi.fn();
  render(<Harness onChange={changed} />);
  const user = await replaceWith("-9");
  await user.keyboard("{Escape}");
  expect(field()).toHaveValue(0.5);
  await user.tab();
  expect(changed).not.toHaveBeenCalled();
});
