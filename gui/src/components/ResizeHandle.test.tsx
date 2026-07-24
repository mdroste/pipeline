import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ResizeHandle from "./ResizeHandle";

describe("ResizeHandle", () => {
  it("supports keyboard resizing within the configured bounds", async () => {
    const user = userEvent.setup();
    const onResize = vi.fn();
    render(
      <div>
        <ResizeHandle
          currentWidth={200}
          defaultWidth={200}
          label="Resize test panel"
          min={160}
          max={240}
          onResize={onResize}
        />
      </div>,
    );

    const divider = screen.getByRole("separator", { name: "Resize test panel" });
    await user.click(divider);
    await user.keyboard("{ArrowRight}{Shift>}{ArrowLeft}{/Shift}{End}{Home}");

    expect(onResize).toHaveBeenNthCalledWith(1, 208);
    expect(onResize).toHaveBeenNthCalledWith(2, 168);
    expect(onResize).toHaveBeenNthCalledWith(3, 240);
    expect(onResize).toHaveBeenNthCalledWith(4, 160);
  });

  it("resets to the default width on double click", () => {
    const onResize = vi.fn();
    render(
      <div>
        <ResizeHandle
          currentWidth={232}
          defaultWidth={200}
          min={160}
          max={240}
          onResize={onResize}
        />
      </div>,
    );

    fireEvent.doubleClick(screen.getByRole("separator"));
    expect(onResize).toHaveBeenCalledWith(200);
  });
});
