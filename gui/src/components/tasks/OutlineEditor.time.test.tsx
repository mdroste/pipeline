import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import OutlineEditor from "./OutlineEditor";
import { template } from "../../lib/taskClient";

it("displays the deadline using its own offset instead of today's DST offset", () => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-07-21T12:00:00Z"));
  const offset = vi
    .spyOn(Date.prototype, "getTimezoneOffset")
    .mockImplementation(function (this: Date) {
      return this.getUTCMonth() === 10 ? 480 : 420;
    });
  try {
    const chain = template("review", "Deadline", "auto", 1);
    chain.steps = [
      {
        kind: "until",
        id: "wait",
        label: "Deadline",
        at: Date.parse("2026-11-21T17:00:00Z") / 1000,
      },
    ];
    render(<OutlineEditor chain={chain} profiles={[]} onChange={vi.fn()} />);
    fireEvent.click(screen.getByTitle("Edit step"));
    expect(screen.getByLabelText("Wait until")).toHaveValue("2026-11-21T09:00");
  } finally {
    offset.mockRestore();
    vi.useRealTimers();
  }
});
