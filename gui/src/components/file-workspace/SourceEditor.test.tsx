import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeAll, expect, it, vi } from "vitest";
import { EditorView } from "@codemirror/view";
import { createRef } from "react";
import SourceEditor, { type SourceEditorHandle } from "./SourceEditor";
import {
  fileLanguage,
  highlightedSource,
  texCompletions,
} from "../../lib/fileLanguages";
beforeAll(() => {
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
});
it("keeps highlighting, edits, navigation and read-only behavior in the same editor", async () => {
  const change = vi.fn();
  const ref = createRef<SourceEditorHandle>();
  const { rerender } = render(
    <SourceEditor
      ref={ref}
      path="estimate.do"
      value={"local x 1\nreg y x"}
      onChange={change}
    />,
  );
  const element = screen.getByRole("textbox", { name: "Source editor" });
  const view = EditorView.findFromDOM(element)!;
  await act(async () => {
    view.dispatch({ changes: { from: 0, to: 5, insert: "global" } });
  });
  expect(change).toHaveBeenCalledWith("global x 1\nreg y x");
  act(() => ref.current?.goToLine(2));
  expect(view.state.selection.main.head).toBe(11);
  rerender(
    <SourceEditor
      ref={ref}
      path="estimate.do"
      value={"global x 1\nreg y x"}
      readOnly
      onChange={change}
    />,
  );
  expect(view.state.readOnly).toBe(true);
  fireEvent.change(screen.getByLabelText("Source language"), {
    target: { value: "python" },
  });
  await waitFor(() =>
    expect(screen.getByLabelText("Source language")).toHaveValue("python"),
  );
});
it("uses explicit academic language grammars and project citation/label completions", () => {
  for (const [path, language] of [
    ["x.do", "stata"],
    ["x.ado", "stata"],
    ["x.R", "r"],
    ["x.jl", "julia"],
    ["x.m", "matlab"],
    ["x.tex", "latex"],
  ])
    expect(fileLanguage(path)).toBe(language);
  expect(highlightedSource("local x 1\nregress y x", "stata")).toContain(
    "hljs-",
  );
  expect(
    texCompletions(["\\label{eq:euler}", "@article{farhi2019, title={Paper}}"]),
  ).toEqual(
    expect.arrayContaining([
      { label: "eq:euler", type: "constant" },
      { label: "farhi2019", type: "keyword" },
    ]),
  );
});

it.each(["\n", "\r\n"])(
  "keeps a second editor's cursor when the same file is edited elsewhere (%j)",
  (separator) => {
    const source = ["one", "two", "three 😀 end"].join(separator);
    const editors = (value: string) => (
      <>
        <SourceEditor path="same.tex" label="First" value={value} />
        <SourceEditor path="same.tex" label="Second" value={value} />
      </>
    );
    const { rerender } = render(editors(source));
    const [first, second] = ["First", "Second"].map((name) =>
      EditorView.findFromDOM(screen.getByRole("textbox", { name }))!,
    );
    const cursor = second.state.doc.line(2).from + 1;
    act(() => second.dispatch({ selection: { anchor: cursor } }));
    act(() => first.dispatch({ changes: { from: 0, insert: "X" } }));
    rerender(editors(first.state.sliceDoc()));
    expect(second.state.sliceDoc()).toBe(`X${source}`);
    expect(second.state.selection.main.head).toBe(cursor + 1);
    // An emoji edit must not split the surrogate pair shared by both texts.
    const emoji = first.state.doc.toString().indexOf("😀");
    act(() =>
      first.dispatch({
        changes: { from: emoji, to: emoji + 2, insert: "😁" },
      }),
    );
    rerender(editors(first.state.sliceDoc()));
    expect(second.state.sliceDoc()).toBe(`X${source}`.replace("😀", "😁"));
    expect(second.state.selection.main.head).toBe(cursor + 1);
  },
);

it("reveals requested lines without taking focus, and focuses only for explicit Go to line", () => {
  const ref = createRef<SourceEditorHandle>();
  const value = "first\nsecond\nthird";
  const { rerender } = render(
    <>
      <input aria-label="Source line" />
      <SourceEditor ref={ref} path="paper.tex" value={value} />
    </>,
  );
  const view = EditorView.findFromDOM(
    screen.getByRole("textbox", { name: "Source editor" }),
  )!;
  const field = screen.getByLabelText("Source line");
  act(() => field.focus());
  expect(view.state.selection.main.head).toBe(0);
  const request = { line: 3 };
  const show = (reveal: { line: number }) => (
    <>
      <input aria-label="Source line" />
      <SourceEditor ref={ref} path="paper.tex" value={value} reveal={reveal} />
    </>
  );
  rerender(show(request));
  expect(view.state.selection.main.head).toBe(13);
  expect(field).toHaveFocus();
  act(() => view.dispatch({ selection: { anchor: 0 } }));
  rerender(show(request));
  expect(view.state.selection.main.head).toBe(0);
  rerender(show({ line: 3 }));
  expect(view.state.selection.main.head).toBe(13);
  expect(field).toHaveFocus();
  act(() => ref.current?.goToLine(2));
  expect(view.state.selection.main.head).toBe(6);
  expect(view.hasFocus).toBe(true);
});

it.each(["\n", "\r\n"])(
  "preserves %j separators and exact Unicode selection offsets",
  async (separator) => {
    const source = `αlpha${separator}βeta 😀${separator}last${separator}`;
    const change = vi.fn();
    const select = vi.fn();
    const { rerender } = render(
      <SourceEditor
        path="unicode.txt"
        value={source}
        onChange={change}
        onSelection={select}
      />,
    );
    const view = EditorView.findFromDOM(
      screen.getByRole("textbox", { name: "Source editor" }),
    )!;
    const second = view.state.doc.line(2);
    const third = view.state.doc.line(3);
    act(() =>
      view.dispatch({ selection: { anchor: second.from, head: third.to } }),
    );
    const selection = select.mock.lastCall![0];
    expect(selection.text).toBe(`βeta 😀${separator}last`);
    expect(source.slice(selection.start, selection.end)).toBe(selection.text);
    const bytes = new TextEncoder().encode(source);
    const start = new TextEncoder().encode(
      source.slice(0, selection.start),
    ).length;
    const end = new TextEncoder().encode(source.slice(0, selection.end)).length;
    expect(new TextDecoder().decode(bytes.slice(start, end))).toBe(
      selection.text,
    );
    act(() =>
      view.dispatch({
        changes: { from: second.from, to: second.from + 1, insert: "B" },
      }),
    );
    const edited = source.replace("β", "B");
    expect(change).toHaveBeenLastCalledWith(edited);
    rerender(
      <SourceEditor
        path="unicode.txt"
        value={edited}
        onChange={change}
        onSelection={select}
      />,
    );
    expect(view.state.sliceDoc()).toBe(edited);
    // External reload may deliberately change the file's newline convention.
    const reloaded = "new\r\nfile\r\n";
    rerender(
      <SourceEditor
        path="unicode.txt"
        value={reloaded}
        onChange={change}
        onSelection={select}
      />,
    );
    expect(view.state.sliceDoc()).toBe(reloaded);
  },
);
