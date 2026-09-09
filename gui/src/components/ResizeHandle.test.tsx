import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import ResizeHandle from "./ResizeHandle";

class TestPointerEvent extends MouseEvent {
  pointerId: number;
  isPrimary: boolean;
  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.pointerId = init.pointerId ?? 1;
    this.isPrimary = init.isPrimary ?? true;
  }
}
beforeEach(() => {
  vi.stubGlobal("PointerEvent", TestPointerEvent);
});
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
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

    const divider = screen.getByRole("separator", {
      name: "Resize test panel",
    });
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

  it("moves a right-hand panel's left edge in the keyboard direction", () => {
    const onResize = vi.fn();
    render(
      <div>
        <ResizeHandle
          edge="left"
          currentWidth={320}
          defaultWidth={320}
          min={240}
          max={440}
          onResize={onResize}
        />
      </div>,
    );
    const divider = screen.getByRole("separator");
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    fireEvent.keyDown(divider, { key: "ArrowRight", shiftKey: true });
    fireEvent.keyDown(divider, { key: "Home" });
    fireEvent.keyDown(divider, { key: "End" });
    expect(onResize.mock.calls.map(([width]) => width)).toEqual([
      328, 288, 240, 440,
    ]);
  });

  it("widens a right-hand panel when its divider is dragged left and clamps on release", () => {
    const onResize = vi.fn();
    const { container } = render(
      <div style={{ width: 320 }}>
        <ResizeHandle
          edge="left"
          currentWidth={320}
          min={240}
          max={440}
          onResize={onResize}
        />
      </div>,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 600 });
    fireEvent.pointerMove(document, { clientX: 550 });
    fireEvent.pointerUp(document);
    expect(onResize).toHaveBeenLastCalledWith(370);
    expect(container.firstElementChild).toHaveStyle({ width: "370px" });
    fireEvent.pointerDown(divider, { clientX: 600 });
    fireEvent.pointerMove(document, { clientX: 100 });
    fireEvent.pointerUp(document);
    expect(onResize).toHaveBeenLastCalledWith(440);
    expect(container.firstElementChild).toHaveStyle({ width: "440px" });
    expect(document.body.style.cursor).toBe("");
    expect(document.body.style.userSelect).toBe("");
  });

  it("updates the panel directly while dragging and commits once on release", () => {
    const onResize = vi.fn();
    const frameCallbacks: FrameRequestCallback[] = [];
    const requestFrame = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        frameCallbacks.push(callback);
        return frameCallbacks.length;
      });
    const cancelFrame = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation(() => {});
    const { container } = render(
      <div style={{ width: 200 }}>
        <ResizeHandle
          currentWidth={200}
          min={160}
          max={240}
          onResize={onResize}
        />
      </div>,
    );

    const panel = container.firstElementChild as HTMLElement;
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 100 });
    fireEvent.pointerMove(document, { clientX: 120 });
    fireEvent.pointerMove(document, { clientX: 130 });

    expect(frameCallbacks).toHaveLength(1);
    expect(onResize).not.toHaveBeenCalled();
    frameCallbacks[0](0);
    expect(panel.style.width).toBe("230px");

    fireEvent.pointerUp(document);
    expect(onResize).toHaveBeenCalledTimes(1);
    expect(onResize).toHaveBeenCalledWith(230);

    requestFrame.mockRestore();
    cancelFrame.mockRestore();
  });
});

it.each(["pointercancel", "lostpointercapture", "escape", "blur", "unmount"])(
  "cancels %s without committing or leaving drag state behind",
  (reason) => {
    const commit = vi.fn();
    const { unmount } = render(
      <div style={{ width: 200 }}>
        <ResizeHandle
          currentWidth={200}
          min={160}
          max={400}
          onResize={commit}
        />
      </div>,
    );
    const divider = screen.getByRole("separator");
    const pane = divider.parentElement!;
    expect(divider).toHaveAttribute("aria-controls", pane.id);
    document.body.style.cursor = "crosshair";
    document.body.style.userSelect = "text";
    fireEvent.pointerDown(divider, { clientX: 100, pointerId: 3 });
    fireEvent.pointerMove(document, { clientX: 150, pointerId: 3 });
    if (reason === "unmount") unmount();
    else if (reason === "escape")
      fireEvent.keyDown(document, { key: "Escape" });
    else if (reason === "blur") fireEvent(window, new Event("blur"));
    else
      fireEvent(
        reason === "lostpointercapture" ? divider : document,
        new PointerEvent(reason, { bubbles: true, pointerId: 3 }),
      );
    fireEvent.pointerUp(document, { pointerId: 3 });
    expect(commit).not.toHaveBeenCalled();
    expect(pane.style.width).toBe("200px");
    expect(document.body.style.cursor).toBe("crosshair");
    expect(document.body.style.userSelect).toBe("text");
    document.body.style.cursor = "";
    document.body.style.userSelect = "";
  },
);

it("offers keyboard sizing without a floating options button and validates the contextual range before applying", async () => {
  const user = userEvent.setup();
  const commit = vi.fn();
  render(
    <div>
      <ResizeHandle
        currentWidth={200}
        min={160}
        max={240}
        defaultWidth={200}
        onResize={commit}
      />
    </div>,
  );
  const divider = screen.getByRole("separator", { name: "Resize panel" });
  expect(
    screen.queryByRole("button", { name: "Resize panel: size options" }),
  ).not.toBeInTheDocument();
  divider.focus();
  await user.keyboard("{Enter}");
  const input = screen.getByRole("spinbutton", { name: "Width in pixels" });
  await user.clear(input);
  await user.type(input, "300");
  expect(screen.getByRole("button", { name: "Apply size" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Reset" }));
  await user.click(screen.getByRole("button", { name: "Increase" }));
  expect(commit).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Apply size" }));
  expect(commit).toHaveBeenCalledExactlyOnceWith(208);
  expect(divider).toHaveFocus();
});

it("reserves fixed siblings and a usable main pane without overwriting the preferred width", () => {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
    function (this: HTMLElement) {
      return {
        width: this.dataset.width ? Number(this.dataset.width) : 0,
        height: 600,
      } as DOMRect;
    },
  );
  const commit = vi.fn();
  render(
    <div data-width="800">
      <div style={{ width: 300 }}>
        <ResizeHandle
          currentWidth={300}
          min={160}
          max={440}
          onResize={commit}
        />
      </div>
      <aside data-width="320" />
      <main style={{ flexGrow: 1 }} />
    </div>,
  );
  const divider = screen.getByRole("separator");
  expect(divider).toHaveAttribute("aria-valuemax", "240");
  expect(divider.parentElement).toHaveStyle({ width: "240px" });
  expect(commit).not.toHaveBeenCalled();
  fireEvent.keyDown(divider, { key: "End" });
  expect(commit).toHaveBeenCalledWith(240);
});

it("uses pointer capture and ignores events from another pointer", () => {
  const commit = vi.fn();
  render(
    <div>
      <ResizeHandle currentWidth={200} min={160} max={400} onResize={commit} />
    </div>,
  );
  const divider = screen.getByRole("separator");
  divider.setPointerCapture = vi.fn();
  divider.hasPointerCapture = () => true;
  divider.releasePointerCapture = vi.fn();
  fireEvent.pointerDown(divider, {
    clientX: 100,
    pointerId: 7,
    pointerType: "touch",
  });
  fireEvent.pointerCancel(document, { pointerId: 8 });
  fireEvent.pointerMove(document, { clientX: 300, pointerId: 8 });
  fireEvent.pointerMove(document, { clientX: 140, pointerId: 7 });
  fireEvent.pointerUp(document, { pointerId: 7 });
  expect(divider.setPointerCapture).toHaveBeenCalledWith(7);
  expect(divider.releasePointerCapture).toHaveBeenCalledWith(7);
  expect(commit).toHaveBeenCalledExactlyOnceWith(240);
});
