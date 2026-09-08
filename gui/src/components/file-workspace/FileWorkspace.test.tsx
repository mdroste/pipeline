import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import FileWorkspace from "./FileWorkspace";
import type { FileAdapter, FilePreview } from "../../lib/fileWorkspaceClient";
vi.mock("./SourceEditor", () => ({
  default: ({
    value,
    onChange,
    label,
  }: {
    value: string;
    onChange: (s: string) => void;
    label: string;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));
vi.mock("../ReportViewer", () => ({
  default: ({ markdown }: { markdown: string }) => <div>{markdown}</div>,
}));
const file = (
  path: string,
  text = "Original",
  hash = "first",
): FilePreview => ({
  path,
  hash,
  text,
  bytes: text.length,
  base64: null,
  mime: "text/plain",
  editable: true,
  truncated: false,
  externalPath: null,
});
beforeEach(() => localStorage.clear());
it("retains per-file drafts across tabs and saves using the loaded hash", async () => {
  const adapter: FileAdapter = {
    id: "test",
    read: vi.fn(async (path) => file(path)),
    save: vi.fn(async () => "second"),
  };
  const initial = { path: "a.py" };
  const { unmount } = render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py", "b.py"]}
      initial={initial}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "A draft" },
  });
  fireEvent.change(screen.getByLabelText("Quick open file"), {
    target: { value: "b.py" },
  });
  fireEvent.keyDown(screen.getByLabelText("Quick open file"), { key: "Enter" });
  fireEvent.change(await screen.findByLabelText("Source editor: b.py"), {
    target: { value: "B draft" },
  });
  fireEvent.click(screen.getByRole("tab", { name: "a.py •" }));
  expect(await screen.findByLabelText("Source editor: a.py")).toHaveValue(
    "A draft",
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(adapter.save).toHaveBeenCalledWith(
      expect.objectContaining({ hash: "first", path: "a.py" }),
      "A draft",
    ),
  );
  await screen.findByText("Saved");
  unmount();
  expect(
    JSON.parse(localStorage.getItem("pipeline.files.draft.test.b.py")!).draft,
  ).toBe("B draft");
});
it("preserves a restored draft and its original hash when the disk changed", async () => {
  localStorage.setItem(
    "pipeline.files.draft.conflict.a.py",
    JSON.stringify({
      file: { path: "a.py", hash: "old", text: "Old" },
      draft: "Mine",
    }),
  );
  const adapter: FileAdapter = {
    id: "conflict",
    read: async () => file("a.py", "Other edit", "new"),
    save: vi.fn(async () => {
      throw new Error("File changed externally");
    }),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  expect(await screen.findByLabelText("Source editor: a.py")).toHaveValue(
    "Mine",
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("alert");
  expect(adapter.save).toHaveBeenCalledWith(
    expect.objectContaining({ hash: "old" }),
    "Mine",
  );
  expect(screen.getByLabelText("Source editor: a.py")).toHaveValue("Mine");
});
it("does not replace edits typed while a save is in flight", async () => {
  let finish!: (hash: string) => void;
  const adapter: FileAdapter = {
    id: "race",
    read: async () => file("a.py"),
    save: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  const input = await screen.findByLabelText("Source editor: a.py");
  fireEvent.change(input, { target: { value: "First edit" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  fireEvent.change(input, { target: { value: "Further edit" } });
  await act(async () => finish("saved"));
  expect(input).toHaveValue("Further edit");
  expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
});

it("preserves newer edits and their retained base while Reload is pending", async () => {
  let finish!: (value: FilePreview) => void;
  const read = vi
    .fn()
    .mockResolvedValueOnce(file("a.py"))
    .mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
  const adapter: FileAdapter = { id: "reload-race", read };
  const { unmount } = render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  const editor = await screen.findByLabelText("Source editor: a.py");
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  fireEvent.change(editor, { target: { value: "New unsaved work" } });
  await act(async () => finish(file("a.py", "Changed on disk", "external")));
  expect(editor).toHaveValue("New unsaved work");
  expect(screen.getByText(/Reload skipped/)).toBeInTheDocument();
  unmount();
  expect(
    JSON.parse(localStorage.getItem("pipeline.files.draft.reload-race.a.py")!),
  ).toEqual({
    file: { path: "a.py", hash: "first", text: "Original" },
    draft: "New unsaved work",
  });
});

it("reloads an unchanged buffer and uses the new disk hash for its next save", async () => {
  const adapter: FileAdapter = {
    id: "reload-clean",
    read: vi
      .fn()
      .mockResolvedValueOnce(file("a.py"))
      .mockResolvedValueOnce(file("a.py", "Changed on disk", "external")),
    save: vi.fn(async () => "saved"),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  const editor = await screen.findByLabelText("Source editor: a.py");
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(editor).toHaveValue("Changed on disk"));
  fireEvent.change(editor, { target: { value: "Edited refreshed file" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(adapter.save).toHaveBeenCalledWith(
      expect.objectContaining({ hash: "external" }),
      "Edited refreshed file",
    ),
  );
});

it("gives tabs scoped panels, manual activation and keyboard closing with draft recovery", async () => {
  const user = (await import("@testing-library/user-event")).default.setup();
  const adapter: FileAdapter = {id:"accessible-tabs", read: vi.fn(async path => file(path)), save: vi.fn()};
  render(<FileWorkspace adapter={adapter} paths={["a.py", "b.py"]} initial={{path:"a.py"}}/>);
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {target:{value:"Preserved draft"}});
  const search = screen.getByRole("textbox", {name:"Quick open file"});
  await user.type(search, "b.py{Enter}"); await screen.findByLabelText("Source editor: b.py");
  const a = screen.getByRole("tab", {name:"a.py •"}); const b = screen.getByRole("tab", {name:"b.py"});
  act(() => b.focus()); await user.keyboard("{ArrowLeft}");
  expect(a).toHaveFocus(); expect(b).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{Enter}"); await screen.findByLabelText("Source editor: a.py");
  expect(screen.getByRole("tabpanel", {name:"a.py •"}).id).toBe(a.getAttribute("aria-controls"));
  expect(a.querySelector("button")).toBeNull();
  await user.keyboard("{Delete}");
  await waitFor(() => expect(b).toHaveFocus());
  expect(screen.queryByRole("tab", {name:"a.py •"})).not.toBeInTheDocument();
  expect(JSON.parse(localStorage.getItem("pipeline.files.draft.accessible-tabs.a.py")!).draft).toBe("Preserved draft");
  await user.type(search, "a.py{Enter}");
  expect(await screen.findByLabelText("Source editor: a.py")).toHaveValue("Preserved draft");
});

it("keeps the last file selection when an earlier read completes late", async () => {
  let finishSlow!: (value: FilePreview) => void;
  const adapter: FileAdapter = {id:"selection-race", read: path => path === "slow.py" ? new Promise(resolve => {finishSlow = resolve;}) : Promise.resolve(file(path)), save: vi.fn()};
  render(<FileWorkspace adapter={adapter} paths={["a.py", "slow.py", "b.py"]} initial={{path:"a.py"}}/>);
  await screen.findByLabelText("Source editor: a.py");
  const search = screen.getByLabelText("Quick open file");
  fireEvent.change(search, {target:{value:"slow.py"}}); fireEvent.keyDown(search, {key:"Enter"});
  fireEvent.change(search, {target:{value:"b.py"}}); fireEvent.keyDown(search, {key:"Enter"});
  await screen.findByLabelText("Source editor: b.py");
  await act(async () => finishSlow(file("slow.py")));
  expect(screen.getByRole("tab", {name:"b.py"})).toHaveAttribute("aria-selected", "true");
  expect(screen.queryByRole("tab", {name:"slow.py"})).not.toBeInTheDocument();
});
