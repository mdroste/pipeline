import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import InfoButton from "./InfoButton";

it("keeps its help in sentence case inside an uppercase heading", () => {
  render(
    <div className="uppercase tracking-wider">
      <InfoButton label="Limits">Applies to each step.</InfoButton>
    </div>,
  );
  fireEvent.click(
    screen.getByRole("button", { name: "More information about Limits" }),
  );
  expect(screen.getByRole("dialog", { name: "Limits help" })).toHaveClass(
    "normal-case",
    "tracking-normal",
  );
});
