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
const confirmDialog = vi.hoisted(() => vi.fn());
vi.mock("../DialogService", () => ({ confirmDialog }));
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
  expect(
    screen.getByText(/not reloaded because you made more edits/),
  ).toBeInTheDocument();
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
  const adapter: FileAdapter = {
    id: "accessible-tabs",
    read: vi.fn(async (path) => file(path)),
    save: vi.fn(),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py", "b.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "Preserved draft" },
  });
  const search = screen.getByRole("textbox", { name: "Quick open file" });
  await user.type(search, "b.py{Enter}");
  await screen.findByLabelText("Source editor: b.py");
  const a = screen.getByRole("tab", { name: "a.py •" });
  const b = screen.getByRole("tab", { name: "b.py" });
  act(() => b.focus());
  await user.keyboard("{ArrowLeft}");
  expect(a).toHaveFocus();
  expect(b).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{Enter}");
  await screen.findByLabelText("Source editor: a.py");
  expect(screen.getByRole("tabpanel", { name: "a.py •" }).id).toBe(
    a.getAttribute("aria-controls"),
  );
  expect(a.querySelector("button")).toBeNull();
  await user.keyboard("{Delete}");
  await waitFor(() => expect(b).toHaveFocus());
  expect(screen.queryByRole("tab", { name: "a.py •" })).not.toBeInTheDocument();
  expect(
    JSON.parse(
      localStorage.getItem("pipeline.files.draft.accessible-tabs.a.py")!,
    ).draft,
  ).toBe("Preserved draft");
  await user.type(search, "a.py{Enter}");
  expect(await screen.findByLabelText("Source editor: a.py")).toHaveValue(
    "Preserved draft",
  );
});

it("keeps the last file selection when an earlier read completes late", async () => {
  let finishSlow!: (value: FilePreview) => void;
  const adapter: FileAdapter = {
    id: "selection-race",
    read: (path) =>
      path === "slow.py"
        ? new Promise((resolve) => {
            finishSlow = resolve;
          })
        : Promise.resolve(file(path)),
    save: vi.fn(),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py", "slow.py", "b.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  await screen.findByLabelText("Source editor: a.py");
  const search = screen.getByLabelText("Quick open file");
  fireEvent.change(search, { target: { value: "slow.py" } });
  fireEvent.keyDown(search, { key: "Enter" });
  fireEvent.change(search, { target: { value: "b.py" } });
  fireEvent.keyDown(search, { key: "Enter" });
  await screen.findByLabelText("Source editor: b.py");
  await act(async () => finishSlow(file("slow.py")));
  expect(screen.getByRole("tab", { name: "b.py" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(
    screen.queryByRole("tab", { name: "slow.py" }),
  ).not.toBeInTheDocument();
});

it("compares a conflicting draft with the current file, then saves against the current hash", async () => {
  localStorage.setItem(
    "pipeline.files.draft.rebase.a.py",
    JSON.stringify({
      file: { path: "a.py", hash: "old", text: "Old" },
      draft: "Mine",
    }),
  );
  const adapter: FileAdapter = {
    id: "rebase",
    read: vi.fn(async () => file("a.py", "Other edit", "new")),
    save: vi.fn(async () => "saved"),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  await screen.findByText(/Restored draft; the file changed externally/);
  fireEvent.click(
    screen.getByRole("button", { name: "Compare with current file" }),
  );
  await screen.findByText(/compares your draft with the current file/);
  expect(screen.getByRole("button", { name: "Changes" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.getByText("Other edit")).toBeInTheDocument();
  await waitFor(() =>
    expect(
      JSON.parse(localStorage.getItem("pipeline.files.draft.rebase.a.py")!),
    ).toEqual({
      file: { path: "a.py", hash: "new", text: "Other edit" },
      draft: "Mine",
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() =>
    expect(adapter.save).toHaveBeenCalledWith(
      expect.objectContaining({ hash: "new" }),
      "Mine",
    ),
  );
});

it("explains a save conflict and offers the comparison", async () => {
  const read = vi
    .fn<FileAdapter["read"]>()
    .mockResolvedValueOnce(file("a.py"))
    .mockResolvedValue(file("a.py", "Other edit", "second"));
  const adapter: FileAdapter = {
    id: "save-conflict",
    read,
    save: vi.fn(async () => {
      throw {
        code: "storage_conflict",
        message: "This file changed outside the editor.",
        recovery: "Refresh the Workbench record and retry your edit.",
      };
    }),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "Draft" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(
    await screen.findByText(/Compare with the current file to keep your draft/),
  ).toBeInTheDocument();
  expect(read).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.queryByText(/Refresh the Workbench record/)).toBeNull();
  expect(
    screen.getByRole("button", { name: "Compare with current file" }),
  ).toBeEnabled();
  expect(screen.getByLabelText("Source editor: a.py")).toHaveValue("Draft");
});

it("asks to stop the running response when a save is refused for an unchanged file", async () => {
  const read = vi.fn(async () => file("a.py"));
  const adapter: FileAdapter = {
    id: "save-busy",
    read,
    save: vi.fn(async () => {
      throw {
        code: "storage_conflict",
        message: "Stop the active research turn before saving files",
        recovery: "Refresh the Workbench record and retry your edit.",
      };
    }),
  };
  render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Source editor: a.py"), {
    target: { value: "Draft" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(
    await screen.findByText("Stop the running response before saving."),
  ).toBeInTheDocument();
  expect(read).toHaveBeenCalledTimes(2);
  expect(screen.queryByText(/changed elsewhere/)).toBeNull();
  expect(screen.queryByText(/Refresh the Workbench record/)).toBeNull();
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.getByLabelText("Source editor: a.py")).toHaveValue("Draft");
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled(),
  );
});

it("reloads a file with unsaved edits only after confirmation", async () => {
  confirmDialog.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
  const read = vi
    .fn()
    .mockResolvedValueOnce(file("a.py"))
    .mockResolvedValueOnce(file("a.py", "Changed on disk", "external"));
  render(
    <FileWorkspace
      adapter={{ id: "reload-dirty", read, save: vi.fn() }}
      paths={["a.py"]}
      initial={{ path: "a.py" }}
    />,
  );
  const editor = await screen.findByLabelText("Source editor: a.py");
  fireEvent.change(editor, { target: { value: "Unsaved" } });
  await act(async () =>
    fireEvent.click(screen.getByRole("button", { name: "Reload" })),
  );
  expect(confirmDialog).toHaveBeenCalledOnce();
  expect(read).toHaveBeenCalledOnce();
  expect(editor).toHaveValue("Unsaved");
  fireEvent.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(editor).toHaveValue("Changed on disk"));
});

it("opens each file tab in a fresh view instead of reusing the previous file's scroll position", async () => {
  const adapter: FileAdapter = {
    id: "fresh-panes",
    read: vi.fn(async (path) => ({
      ...file(path, `# ${path}`),
      editable: false,
    })),
  };
  const { container } = render(
    <FileWorkspace
      adapter={adapter}
      paths={["a.md", "b.md"]}
      initial={{ path: "a.md" }}
    />,
  );
  await screen.findByText("# a.md");
  const first = container.querySelector(".file-pane");
  fireEvent.change(screen.getByLabelText("Quick open file"), {
    target: { value: "b.md" },
  });
  fireEvent.keyDown(screen.getByLabelText("Quick open file"), {
    key: "Enter",
  });
  await screen.findByText("# b.md");
  expect(container.querySelector(".file-pane")).not.toBe(first);
});
