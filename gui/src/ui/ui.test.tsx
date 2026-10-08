import { useRef, useState } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { computeAnchoredPosition } from "./anchoredPosition";
import Popover from "./Popover";
import { Menu, MenuItem } from "./Menu";
import Select from "./Select";
import SegmentedControl from "./SegmentedControl";
import Tooltip from "./Tooltip";
import IconButton from "./IconButton";
import Sheet from "./Sheet";
import Skeleton from "./Skeleton";

const viewport = { width: 1000, height: 800 };
const anchor = (left: number, top: number, width = 100, height = 30) => ({
  left,
  top,
  width,
  right: left + width,
  bottom: top + height,
});

describe("computeAnchoredPosition", () => {
  it("places below and start-aligned when there is room", () => {
    expect(
      computeAnchoredPosition(
        anchor(200, 100),
        { width: 240, height: 200 },
        viewport,
      ),
    ).toMatchObject({ left: 200, top: 138, side: "bottom" });
  });
  it("flips to the other side when the layer does not fit", () => {
    const below = computeAnchoredPosition(
      anchor(200, 700),
      { width: 240, height: 300 },
      viewport,
      "bottom",
    );
    expect(below.side).toBe("top");
    expect(below.top).toBe(700 - 8 - 300);
    const above = computeAnchoredPosition(
      anchor(200, 40),
      { width: 240, height: 300 },
      viewport,
      "top",
    );
    expect(above.side).toBe("bottom");
  });
  it("keeps the layer inside the window and caps its height", () => {
    const end = computeAnchoredPosition(
      anchor(10, 100, 40),
      { width: 300, height: 2000 },
      viewport,
      "bottom",
      "end",
    );
    expect(end.left).toBe(12);
    expect(end.maxHeight).toBe(800 - 130 - 8 - 12);
    const right = computeAnchoredPosition(
      anchor(950, 100, 40),
      { width: 300, height: 100 },
      viewport,
    );
    expect(right.left).toBe(1000 - 300 - 12);
  });
});

function PopoverHarness({ focusOut = false }: { focusOut?: boolean }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const inner = useRef<HTMLButtonElement>(null);
  const [nested, setNested] = useState(false);
  return (
    <>
      <button ref={trigger} onClick={() => setOpen((value) => !value)}>
        Open
      </button>
      <button>Elsewhere</button>
      <Popover
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label="Details"
        closeOnFocusOut={focusOut}
      >
        <button ref={inner} onClick={() => setNested(true)}>
          Inside
        </button>
        <Popover
          open={nested}
          onClose={() => setNested(false)}
          anchorRef={inner}
          label="Nested"
        >
          <button>Deep</button>
        </Popover>
      </Popover>
    </>
  );
}

describe("Popover", () => {
  it("focuses its content, closes on Escape, and returns focus", () => {
    render(<PopoverHarness />);
    const trigger = screen.getByRole("button", { name: "Open" });
    fireEvent.click(trigger);
    expect(screen.getByRole("button", { name: "Inside" })).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
  it("closes on an outside press but not on a press inside a nested layer", () => {
    render(<PopoverHarness />);
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.click(screen.getByRole("button", { name: "Inside" }));
    fireEvent.pointerDown(screen.getByRole("button", { name: "Deep" }));
    expect(screen.getByRole("dialog", { name: "Details" })).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Nested" })).toBeInTheDocument();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Elsewhere" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("closes when focus leaves only if asked to", () => {
    const { unmount } = render(<PopoverHarness />);
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.focusIn(screen.getByRole("button", { name: "Elsewhere" }));
    expect(screen.getByRole("dialog", { name: "Details" })).toBeInTheDocument();
    unmount();
    render(<PopoverHarness focusOut />);
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    fireEvent.focusIn(screen.getByRole("button", { name: "Elsewhere" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

function MenuHarness({ onSelect }: { onSelect: (id: string) => void }) {
  const trigger = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  return (
    <>
      <button ref={trigger} onClick={() => setOpen((value) => !value)}>
        Actions
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={trigger}
        label="Actions"
      >
        <MenuItem onSelect={() => onSelect("rename")}>Rename</MenuItem>
        <MenuItem disabled onSelect={() => onSelect("move")}>
          Move
        </MenuItem>
        <MenuItem checked onSelect={() => onSelect("archive")}>
          Archive
        </MenuItem>
        <MenuItem danger onSelect={() => onSelect("delete")}>
          Delete
        </MenuItem>
      </Menu>
    </>
  );
}

describe("Menu", () => {
  it("moves with arrow keys and type-ahead, skipping disabled items", () => {
    render(<MenuHarness onSelect={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Actions" }));
    const menu = screen.getByRole("menu", { name: "Actions" });
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(
      screen.getByRole("menuitemradio", { name: "Archive" }),
    ).toHaveFocus();
    fireEvent.keyDown(menu, { key: "End" });
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "Rename" })).toHaveFocus();
    fireEvent.keyDown(menu, { key: "d" });
    expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
  });
  it("runs the chosen action, closes, and returns focus to the trigger", () => {
    const select = vi.fn();
    render(<MenuHarness onSelect={select} />);
    const trigger = screen.getByRole("button", { name: "Actions" });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(select).toHaveBeenCalledWith("delete");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
});

describe("Select", () => {
  const options = [
    { value: "a", label: "Alpha", description: "First" },
    { value: "b", label: "Beta", disabled: true },
    { value: "c", label: "Gamma" },
  ];
  it("shows the current choice and changes it from the list", () => {
    const change = vi.fn();
    render(
      <Select label="Project" value="a" options={options} onChange={change} />,
    );
    const trigger = screen.getByRole("combobox", { name: "Project" });
    expect(trigger).toHaveTextContent("Alpha");
    fireEvent.click(trigger);
    expect(screen.getByRole("option", { name: /Alpha/ })).toHaveFocus();
    expect(screen.getByRole("option", { name: /Alpha/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    fireEvent.click(screen.getByRole("option", { name: "Beta" }));
    expect(change).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "ArrowDown" });
    expect(screen.getByRole("option", { name: "Gamma" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Enter" });
    expect(change).toHaveBeenCalledWith("c");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });
  it("opens from the keyboard and shows an unknown saved value as is", () => {
    render(
      <Select
        label="Project"
        value="gone"
        options={options}
        onChange={vi.fn()}
      />,
    );
    const trigger = screen.getByRole("combobox", { name: "Project" });
    expect(trigger).toHaveTextContent("gone");
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(screen.getByRole("listbox")).toBeInTheDocument();
  });
});

describe("SegmentedControl", () => {
  it("chooses by click and by arrow key, skipping disabled segments", () => {
    const change = vi.fn();
    const view = render(
      <SegmentedControl
        label="Thinking"
        value="low"
        onChange={change}
        options={[
          { value: "low", label: "Low" },
          { value: "medium", label: "Medium", disabled: true },
          { value: "high", label: "High" },
        ]}
      />,
    );
    expect(screen.getByRole("radio", { name: "Low" })).toBeChecked();
    fireEvent.keyDown(screen.getByRole("radiogroup"), { key: "ArrowRight" });
    expect(change).toHaveBeenLastCalledWith("high");
    fireEvent.click(screen.getByRole("radio", { name: "High" }));
    expect(change).toHaveBeenCalledTimes(2);
    view.rerender(
      <SegmentedControl
        label="Thinking"
        value="low"
        disabled
        onChange={change}
        options={[{ value: "low", label: "Low" }]}
      />,
    );
    expect(screen.getByRole("radio", { name: "Low" })).toBeDisabled();
  });
});

describe("Tooltip and IconButton", () => {
  afterEach(() => vi.useRealTimers());
  it("names the button and describes it after a hover delay", () => {
    vi.useFakeTimers();
    const click = vi.fn();
    render(
      <IconButton label="Attach" tooltip="Add files" onClick={click}>
        +
      </IconButton>,
    );
    const button = screen.getByRole("button", { name: "Attach" });
    fireEvent.mouseEnter(button);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    act(() => void vi.advanceTimersByTime(400));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Add files");
    expect(button).toHaveAccessibleDescription("Add files");
    fireEvent.mouseLeave(button);
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    fireEvent.click(button);
    expect(click).toHaveBeenCalledOnce();
  });
  it("keeps the child's own handlers and ref", () => {
    const enter = vi.fn();
    const ref = { current: null as HTMLButtonElement | null };
    render(
      <Tooltip label="More">
        <button ref={ref} onMouseEnter={enter}>
          Child
        </button>
      </Tooltip>,
    );
    fireEvent.mouseEnter(screen.getByRole("button", { name: "Child" }));
    expect(enter).toHaveBeenCalledOnce();
    expect(ref.current).toBe(screen.getByRole("button", { name: "Child" }));
  });
});

describe("Sheet and Skeleton", () => {
  it("closes from Escape, the close button, and the scrim, restoring focus", () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <div className="relative">
          <button onClick={() => setOpen(true)}>Settings</button>
          <Sheet open={open} onClose={() => setOpen(false)} title="Settings">
            <p>Body</p>
          </Sheet>
        </div>
      );
    }
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Settings" });
    trigger.focus();
    fireEvent.click(trigger);
    const sheet = screen.getByRole("dialog", { name: "Settings" });
    expect(sheet).toHaveFocus();
    fireEvent.keyDown(sheet, { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    fireEvent.pointerDown(screen.getByTestId("sheet-scrim"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("labels what is loading", () => {
    render(<Skeleton label="Loading sources" lines={2} />);
    expect(
      screen.getByRole("status", { name: "Loading sources" }),
    ).toBeInTheDocument();
  });
});
