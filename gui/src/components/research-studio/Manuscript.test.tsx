import { flushDrafts } from "../../lib/flushDrafts";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import Manuscript, { buildFolder } from "./Manuscript";
import { suggestBuild } from "./ManuscriptBuildSheet";
import type { ProjectHome } from "../../lib/projectClient";
import type { SourceLineRequest } from "../file-workspace/SourceEditor";
const mocks = vi.hoisted(() => ({
  records: vi.fn(),
  editor: vi.fn(),
  mutate: vi.fn(),
  sync: vi.fn(),
  listExecutionProfiles: vi.fn(),
  detectToolchains: vi.fn(),
  previewExecution: vi.fn(),
  projectMutate: vi.fn(),
  goToLine: vi.fn(),
  pdf: vi.fn(),
  editorProps: [] as Array<{ path: string; reveal?: SourceLineRequest | null }>,
  confirmDialog: vi.fn(),
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: mocks,
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: mocks,
}));
vi.mock("../../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../../lib/projectClient")>()),
  projectClient: {
    previewExecution: mocks.previewExecution,
    mutate: mocks.projectMutate,
  },
}));
vi.mock("../DialogService", () => ({ confirmDialog: mocks.confirmDialog }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../file-workspace/PdfComparison", () => ({ default: () => null }));
vi.mock("../file-workspace/PdfReader", () => ({
  default: (props: unknown) => {
    mocks.pdf(props);
    return null;
  },
}));
vi.mock("../file-workspace/SourceEditor", async () => {
  const { forwardRef, useImperativeHandle } = await import("react");
  return {
    default: forwardRef(function SourceEditor(
      props: {
        label: string;
        value: string;
        path: string;
        readOnly?: boolean;
        reveal?: SourceLineRequest | null;
        onChange?: (value: string) => void;
      },
      ref,
    ) {
      useImperativeHandle(ref, () => ({
        goToLine: mocks.goToLine,
        focus: vi.fn(),
      }));
      mocks.editorProps.push(props);
      return (
        <textarea
          aria-label={props.label}
          value={props.value}
          readOnly={props.readOnly}
          onChange={(event) => props.onChange?.(event.target.value)}
        />
      );
    }),
  };
});
const data = {
  settings: { body: { manuscriptRevisionId: null } },
  papers: [],
  changes: [{ id: "checkpoint", body: { state: "isolated", taskId: "task" } }],
  tasks: [{ id: "task", body: { objective: "Revise the introduction" } }],
  applications: [],
  anchors: [],
  executions: [],
} as unknown as ProjectHome;
const editorProps = () => mocks.editorProps[mocks.editorProps.length - 1];
beforeEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
  mocks.editorProps = [];
  mocks.projectMutate.mockResolvedValue({});
  mocks.listExecutionProfiles.mockResolvedValue([]);
  mocks.detectToolchains.mockResolvedValue({ tools: [] });
  mocks.previewExecution.mockResolvedValue({
    command: ["latexmk", "main.tex"],
    cwd: "/paper",
    inputs: {},
    launch: {},
    outputs: ["main.pdf"],
  });
  mocks.editor.mockResolvedValue({
    path: "main.tex",
    content: "\\section{Intro}",
    hash: "hash",
    checkpointId: null,
  });
  mocks.records.mockResolvedValue([
    {
      id: "build_1",
      revision: 1,
      body: {
        recordType: "receipt",
        executionId: "execution",
        outcome: "succeeded",
        diagnostics: [
          {
            severity: "warning",
            message: "Overfull",
            path: "main.tex",
            line: 7,
          },
        ],
        pdf: { revision: { id: "pdf-revision" } },
        mappingArtifactId: "mapping",
        pageInspection: "",
        synchronization: "",
        sourceManifest: {},
      },
    },
  ]);
});

it("moves the editor only on explicit source actions, never while typing a line number", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  expect(editorProps()).toMatchObject({
    path: "ws/working/main.tex",
    reveal: null,
  });
  await screen.findByRole("button", { name: /^Last build succeeded/ });
  fireEvent.click(screen.getByRole("button", { name: "Builds" }));
  const line = screen.getByLabelText("Source line");
  fireEvent.change(line, { target: { value: "" } });
  expect(screen.getByRole("button", { name: "Go to line" })).toBeDisabled();
  fireEvent.change(line, { target: { value: "4" } });
  fireEvent.change(line, { target: { value: "42" } });
  expect(line).toHaveValue(42);
  expect(editorProps().reveal).toBeNull();
  expect(mocks.goToLine).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Go to line" }));
  expect(mocks.goToLine).toHaveBeenCalledWith(42);
  fireEvent.click(screen.getByRole("button", { name: "main.tex:7" }));
  await waitFor(() => expect(editorProps().reveal).toEqual({ line: 7 }));
  expect(line).toHaveValue(7);
  expect(mocks.goToLine).toHaveBeenCalledOnce();
});

it("opens a found source location in its own file, at that line", async () => {
  mocks.sync.mockResolvedValue({
    candidates: [{ sourcePath: "chapters/intro.tex", line: 12 }],
  });
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  await screen.findByRole("button", { name: /^Last build succeeded/ });
  fireEvent.click(screen.getByRole("button", { name: "Builds" }));
  fireEvent.change(screen.getByLabelText("Source line"), {
    target: { value: "3" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Find line in PDF" }));
  mocks.editor.mockResolvedValue({
    path: "chapters/intro.tex",
    content: "Intro",
    hash: "intro",
    checkpointId: null,
  });
  fireEvent.click(
    await screen.findByRole("button", { name: "Source chapters/intro.tex:12" }),
  );
  await waitFor(() =>
    expect(screen.getByLabelText("Manuscript source editor")).toHaveValue(
      "Intro",
    ),
  );
  expect(editorProps()).toMatchObject({
    path: "ws/working/chapters/intro.tex",
    reveal: { line: 12 },
  });
});

it("opens the project's main file and the latest build by itself", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={
        {
          ...data,
          inventory: {
            body: { files: [{ path: "notes.md" }, { path: "paper/main.tex" }] },
          },
        } as unknown as ProjectHome
      }
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  expect(mocks.editor).toHaveBeenCalledWith("ws", "paper/main.tex", null);
  expect(
    await screen.findByRole("button", { name: /^Last build succeeded/ }),
  ).toBeInTheDocument();
  expect(screen.getByRole("group", { name: "Side panel" })).toBeInTheDocument();
});

it("starts on the file menu when the project has no file to open", async () => {
  mocks.editor.mockRejectedValue(new Error("missing"));
  mocks.records.mockResolvedValue([]);
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  expect(await screen.findByText("No manuscript file open")).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("inspects a finished build without being asked", async () => {
  mocks.records.mockResolvedValue([]);
  mocks.mutate.mockResolvedValue({});
  const onRefresh = vi.fn();
  render(
    <Manuscript
      workspaceId="ws"
      data={
        {
          ...data,
          executions: [{ id: "new", adapter: "latex", outcome: "completed" }],
        } as unknown as ProjectHome
      }
      onRefresh={onRefresh}
      onDocument={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith("ws", {
      action: "inspectBuild",
      executionId: "new",
    }),
  );
  await waitFor(() => expect(onRefresh).toHaveBeenCalled());
  expect(mocks.mutate).toHaveBeenCalledOnce();
});

it("starts a first build with automatic dependencies instead of a stale file list", () => {
  expect(suggestBuild("paper/main.tex")).toMatchObject({
    directory: "paper",
    rootDocument: "main.tex",
    expectedPdf: "main.pdf",
    inputs: ["main.tex"],
    automaticDependencies: true,
  });
});

it("stays on Builds when an older build is chosen", async () => {
  const receipt = (id: string, executionId: string) => ({
    id,
    revision: 1,
    body: {
      recordType: "receipt",
      executionId,
      outcome: "completed",
      diagnostics: [],
      pdf: { revision: { id: `pdf-${id}` } },
      sourceManifest: {},
    },
  });
  mocks.records.mockResolvedValue([
    receipt("build_new", "new"),
    receipt("build_old", "old"),
  ]);
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Builds" }));
  const list = await screen.findByRole("list", { name: "Builds" });
  fireEvent.click(within(list).getAllByRole("button")[1]);
  expect(within(list).getAllByRole("button")[1]).toHaveAttribute(
    "aria-current",
    "true",
  );
  expect(screen.getByRole("list", { name: "Builds" })).toBeInTheDocument();
});

it("reads a build's paths from its build folder", () => {
  expect(buildFolder("/project/paper", ["paper", "."])).toBe("paper/");
  expect(buildFolder("C:\\project\\paper\\draft", ["paper/draft"])).toBe(
    "paper/draft/",
  );
  expect(buildFolder("/project", ["."])).toBe("");
  expect(buildFolder(undefined, ["paper"])).toBe("");
});

const openManuscript = async (active = true) => {
  const view = (visible: boolean) => (
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
      active={visible}
    />
  );
  const rendered = render(view(active));
  const editor = await screen.findByLabelText("Manuscript source editor");
  await waitFor(() => expect(editor).toHaveValue("\\section{Intro}"));
  return { ...rendered, editor, view };
};
const onDisk = (content: string, hash: string) => ({
  path: "main.tex",
  content,
  hash,
  checkpointId: null,
});

it("follows the file when the page comes back with no draft open", async () => {
  const { editor, rerender, view } = await openManuscript();
  rerender(view(false));
  mocks.editor.mockResolvedValue(onDisk("Accepted edit", "accepted"));
  rerender(view(true));
  await waitFor(() => expect(editor).toHaveValue("Accepted edit"));
  expect(screen.getByRole("status")).toHaveTextContent(
    "Updated from the project",
  );
});

it("keeps a draft when the file changes, and Compare lets it save", async () => {
  const { editor } = await openManuscript();
  fireEvent.change(editor, { target: { value: "My draft" } });
  mocks.editor.mockResolvedValue(onDisk("Accepted edit", "accepted"));
  fireEvent(window, new Event("focus"));
  fireEvent.click(
    await screen.findByRole("button", { name: "Compare with current file" }),
  );
  await waitFor(() =>
    expect(screen.getByRole("status")).toHaveTextContent(
      "Changes now compares your draft",
    ),
  );
  expect(editor).toHaveValue("My draft");
  mocks.mutate.mockResolvedValue({ hash: "saved" });
  fireEvent.click(
    screen.getByRole("button", { name: /^Save (working copy|task draft)$/ }),
  );
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith(
      "ws",
      expect.objectContaining({
        action: "saveText",
        expectedHash: "accepted",
        content: "My draft",
      }),
    ),
  );
});

it("turns a refused save into a way forward, not a dead end", async () => {
  const { editor } = await openManuscript();
  fireEvent.change(editor, { target: { value: "My draft" } });
  mocks.mutate.mockRejectedValueOnce({
    code: "storage_conflict",
    message: "This file changed outside the editor.",
  });
  mocks.editor.mockResolvedValue(onDisk("Accepted edit", "accepted"));
  fireEvent.click(
    screen.getByRole("button", { name: /^Save (working copy|task draft)$/ }),
  );
  expect(
    await screen.findByRole("button", { name: "Compare with current file" }),
  ).toBeVisible();
  expect(screen.getByRole("status")).toHaveTextContent(
    "This file changed outside the editor",
  );
  expect(screen.queryByRole("alert")).toBeNull();
  expect(editor).toHaveValue("My draft");
});

it("reports a save refused while a response runs as that, not as a change", async () => {
  const { editor } = await openManuscript();
  fireEvent.change(editor, { target: { value: "My draft" } });
  mocks.mutate.mockRejectedValueOnce({
    code: "storage_conflict",
    message: "Stop the running response before saving.",
  });
  fireEvent.click(
    screen.getByRole("button", { name: /^Save (working copy|task draft)$/ }),
  );
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Stop the running response",
  );
  expect(
    screen.queryByRole("button", { name: "Compare with current file" }),
  ).toBeNull();
});

it("discards and reloads a draft only after asking", async () => {
  const { editor } = await openManuscript();
  fireEvent.change(editor, { target: { value: "My draft" } });
  mocks.confirmDialog.mockResolvedValueOnce(false);
  fireEvent.click(
    screen.getByRole("button", { name: "More manuscript actions" }),
  );
  fireEvent.click(
    screen.getByRole("menuitem", { name: "Discard unsaved draft" }),
  );
  await waitFor(() => expect(mocks.confirmDialog).toHaveBeenCalledOnce());
  expect(editor).toHaveValue("My draft");

  mocks.confirmDialog.mockResolvedValueOnce(true);
  mocks.editor.mockResolvedValue(onDisk("Accepted edit", "accepted"));
  fireEvent.click(
    screen.getByRole("button", { name: "More manuscript actions" }),
  );
  fireEvent.click(screen.getByRole("menuitem", { name: "Reload file" }));
  await waitFor(() => expect(editor).toHaveValue("Accepted edit"));
  expect(mocks.confirmDialog).toHaveBeenCalledTimes(2);
  expect(localStorage.getItem("pipeline.manuscriptDraft.ws")).toBeNull();
});

it("keeps the original copy selected if opening another copy fails", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  mocks.editor.mockRejectedValueOnce(new Error("Copy unavailable"));
  fireEvent.change(screen.getByLabelText("Edit in"), {
    target: { value: "checkpoint" },
  });
  await screen.findByText("Copy unavailable");
  expect(screen.getByLabelText("Edit in")).toHaveValue("");
  expect(editorProps().path).toBe("ws/working/main.tex");
});

it("commits the copy selection with its file and blocks concurrent switching", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  let resolve!: (v: unknown) => void;
  mocks.editor.mockReturnValueOnce(
    new Promise((r) => {
      resolve = r;
    }),
  );
  fireEvent.change(screen.getByLabelText("Edit in"), {
    target: { value: "checkpoint" },
  });
  expect(screen.getByLabelText("Edit in")).toBeDisabled();
  fireEvent.change(screen.getByLabelText("Edit in"), { target: { value: "" } });
  await act(async () =>
    resolve({
      path: "main.tex",
      content: "Copy text",
      hash: "copy",
      checkpointId: "checkpoint",
    }),
  );
  expect(screen.getByLabelText("Edit in")).toHaveValue("checkpoint");
  expect(editorProps().path).toBe("ws/checkpoint/main.tex");
  mocks.mutate.mockResolvedValueOnce({ hash: "saved" });
  fireEvent.change(screen.getByLabelText("Manuscript source editor"), {
    target: { value: "Copy draft" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save task draft" }));
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenCalledWith(
      "ws",
      expect.objectContaining({
        checkpointId: "checkpoint",
        content: "Copy draft",
      }),
    ),
  );
});

it("retains the latest manuscript draft on pagehide before the debounce", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  const editor = await screen.findByLabelText("Manuscript source editor");
  fireEvent.change(editor, { target: { value: "Latest unsaved words" } });
  fireEvent(window, new Event("pagehide"));
  expect(
    JSON.parse(
      localStorage.getItem("pipeline.manuscriptDraft.ws.working.main.tex")!,
    ),
  ).toMatchObject({ draft: "Latest unsaved words" });
});

it("keeps edits delivered after discard confirmation while the file is loading", async () => {
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  const editor = await screen.findByLabelText("Manuscript source editor");
  fireEvent.change(editor, { target: { value: "Draft I agreed to discard" } });
  mocks.confirmDialog.mockResolvedValue(true);
  let resolve!: (v: unknown) => void;
  mocks.editor.mockReturnValueOnce(
    new Promise((r) => {
      resolve = r;
    }),
  );
  const reads = mocks.editor.mock.calls.length;
  fireEvent.click(
    screen.getByRole("button", { name: "More manuscript actions" }),
  );
  fireEvent.click(
    screen.getByRole("menuitem", { name: "Discard unsaved draft" }),
  );
  await waitFor(() => expect(mocks.editor).toHaveBeenCalledTimes(reads + 1));
  expect(editor).toHaveAttribute("readonly");
  fireEvent.change(editor, {
    target: { value: "NEW words typed after confirming" },
  });
  await act(async () =>
    resolve({
      path: "main.tex",
      content: "Saved source",
      hash: "hash",
      checkpointId: null,
    }),
  );
  expect(editor).toHaveValue("NEW words typed after confirming");
});

it("flushes the current draft before native exit and blocks exit when retention fails", async () => {
  const { editor } = await openManuscript();
  fireEvent.change(editor, { target: { value: "Just typed" } });
  act(() => flushDrafts());
  expect(
    JSON.parse(
      localStorage.getItem("pipeline.manuscriptDraft.ws.working.main.tex")!,
    ),
  ).toMatchObject({ draft: "Just typed" });
  const write = vi.spyOn(localStorage, "setItem").mockImplementation(() => {
    throw new DOMException("Full", "QuotaExceededError");
  });
  fireEvent.change(editor, { target: { value: "Not retained yet" } });
  act(() => expect(() => flushDrafts()).toThrow("Save your files"));
  expect(editor).toHaveValue("Not retained yet");
  // Once the user saves, a denied cleanup alone must not trap them in the app.
  const remove = vi.spyOn(localStorage, "removeItem").mockImplementation(() => {
    throw new Error("Storage unavailable");
  });
  try {
    mocks.mutate.mockResolvedValueOnce({ hash: "saved" });
    fireEvent.click(screen.getByRole("button", { name: "Save working copy" }));
    await screen.findByText(/Saved. Rebuild the manuscript/);
    act(() => expect(() => flushDrafts()).not.toThrow());
  } finally {
    remove.mockRestore();
  }
  write.mockRestore();
});

it("keeps the open copy identified when it leaves the list of available edits", async () => {
  const { rerender } = await openManuscript();
  mocks.editor.mockResolvedValueOnce({
    ...onDisk("Copy", "copy-hash"),
    checkpointId: "checkpoint",
  });
  fireEvent.change(screen.getByLabelText("Edit in"), {
    target: { value: "checkpoint" },
  });
  await waitFor(() =>
    expect(editorProps().path).toBe("ws/checkpoint/main.tex"),
  );
  rerender(
    <Manuscript
      workspaceId="ws"
      data={{ ...data, changes: [] }}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  expect(screen.getByLabelText("Edit in")).toHaveValue("checkpoint");
  expect(
    screen.getByRole("option", { name: /no longer available/ }),
  ).toBeDisabled();
});

const buildSettings = (
  id: string,
  rootDocument = "main.tex",
  engine = "latexmk",
) => ({
  id,
  revision: 1,
  body: {
    recordType: "configuration",
    config: {
      name: "Paper",
      checkpointId: null,
      directory: ".",
      rootDocument,
      expectedPdf: "main.pdf",
      engine,
      timeoutSeconds: 120,
      inputs: [rootDocument],
      advancedArgv: null,
      automaticDependencies: true,
      executable: null,
    },
    profile: { id: `profile_${id}`, revision: 1, testStatus: "passed" },
  },
});

it("restores the selected root and engine when returning to a project", async () => {
  localStorage.setItem("pipeline.manuscriptBuild.ws", "chosen");
  mocks.records.mockResolvedValue([
    buildSettings("latest"),
    buildSettings("chosen", "appendix.tex", "xelatex"),
  ]);
  mocks.editor.mockResolvedValue({
    path: "appendix.tex",
    content: "Appendix",
    hash: "appendix",
    checkpointId: null,
  });
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  expect(mocks.editor).toHaveBeenCalledWith("ws", "appendix.tex", null);
  fireEvent.click(screen.getByRole("button", { name: "Build" }));
  await screen.findByText("xelatex · appendix.tex");
  await waitFor(() =>
    expect(mocks.previewExecution).toHaveBeenCalledWith("profile_chosen"),
  );
});

it("saves open and retained drafts before reviewing the build", async () => {
  mocks.records.mockResolvedValue([buildSettings("paper")]);
  mocks.mutate.mockResolvedValue({ hash: "saved" });
  const retained = {
    file: {
      path: "section.tex",
      hash: "section",
      content: "old",
      checkpointId: null,
    },
    draft: "new section",
  };
  localStorage.setItem(
    "pipeline.manuscriptDraft.ws.working.section.tex",
    JSON.stringify(retained),
  );
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  fireEvent.change(await screen.findByLabelText("Manuscript source editor"), {
    target: { value: "new main" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save and build" }));
  await waitFor(() => expect(mocks.previewExecution).toHaveBeenCalled());
  expect(mocks.mutate).toHaveBeenNthCalledWith(
    1,
    "ws",
    expect.objectContaining({
      action: "saveText",
      path: "main.tex",
      content: "new main",
    }),
  );
  expect(mocks.mutate).toHaveBeenNthCalledWith(
    2,
    "ws",
    expect.objectContaining({
      action: "saveText",
      path: "section.tex",
      content: "new section",
    }),
  );
  expect(mocks.previewExecution.mock.invocationCallOrder[0]).toBeGreaterThan(
    mocks.mutate.mock.invocationCallOrder[1],
  );
  expect(
    localStorage.getItem("pipeline.manuscriptDraft.ws.working.section.tex"),
  ).toBeNull();
});

it("keeps a conflicting retained draft and never prepares its build", async () => {
  mocks.records.mockResolvedValue([buildSettings("paper")]);
  mocks.mutate.mockRejectedValue(new Error("section.tex changed; reopen it"));
  const retained = JSON.stringify({
    file: {
      path: "section.tex",
      hash: "section",
      content: "old",
      checkpointId: null,
    },
    draft: "new section",
  });
  localStorage.setItem(
    "pipeline.manuscriptDraft.ws.working.section.tex",
    retained,
  );
  render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={vi.fn()}
      onDocument={vi.fn()}
    />,
  );
  await screen.findByLabelText("Manuscript source editor");
  fireEvent.click(screen.getByRole("button", { name: "Build" }));
  await screen.findByText(/section.tex changed; reopen it/);
  expect(mocks.previewExecution).not.toHaveBeenCalled();
  expect(
    localStorage.getItem("pipeline.manuscriptDraft.ws.working.section.tex"),
  ).toBe(retained);
});

it("keeps the PDF page and zoom when a completed build replaces the preview", async () => {
  const props = {
    workspaceId: "ws",
    data,
    onRefresh: vi.fn(),
    onDocument: vi.fn(),
  };
  const view = render(<Manuscript {...props} />);
  await waitFor(() => expect(mocks.pdf).toHaveBeenCalled());
  act(() =>
    mocks.pdf.mock.lastCall![0].onPosition({
      page: 4,
      scale: "1.5",
      rotation: 0,
    }),
  );
  const next = {
    id: "build_next",
    revision: 1,
    body: {
      recordType: "receipt",
      executionId: "next",
      outcome: "completed",
      diagnostics: [],
      pdf: { revision: { id: "next-pdf" } },
    },
  };
  mocks.records.mockResolvedValue([next]);
  view.rerender(
    <Manuscript
      {...props}
      data={{
        ...data,
        executions: [
          { id: "next", adapter: "latex", outcome: "completed" },
        ] as ProjectHome["executions"],
      }}
    />,
  );
  await waitFor(() =>
    expect(mocks.pdf.mock.lastCall![0]).toMatchObject({
      documentKey: "ws:next-pdf",
      position: { page: 4, scale: "1.5" },
    }),
  );
});

it("loads an inventory for a newly attached project before listing manuscript files", async () => {
  const onRefresh = vi.fn();
  const view = render(
    <Manuscript
      workspaceId="ws"
      data={data}
      onRefresh={onRefresh}
      onDocument={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(mocks.projectMutate).toHaveBeenCalledWith("ws", {
      action: "checkForChanges",
    }),
  );
  await waitFor(() => expect(onRefresh).toHaveBeenCalled());
  view.rerender(
    <Manuscript
      workspaceId="ws"
      data={
        {
          ...data,
          inventory: {
            body: {
              files: [{ path: "main.tex" }, { path: "sections/intro.tex" }],
            },
          },
        } as ProjectHome
      }
      onRefresh={onRefresh}
      onDocument={vi.fn()}
    />,
  );
  expect(
    await screen.findByRole("option", { name: "sections/intro.tex" }),
  ).toBeInTheDocument();
  expect(mocks.projectMutate).toHaveBeenCalledOnce();
});
