import {
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
  goToLine: vi.fn(),
  editorProps: [] as Array<{ path: string; reveal?: SourceLineRequest | null }>,
}));
vi.mock("../../lib/studioClient", async (original) => ({
  ...(await original<typeof import("../../lib/studioClient")>()),
  studioClient: mocks,
}));
vi.mock("../../lib/workbench/executionClient", () => ({
  executionClient: mocks,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("../file-workspace/PdfComparison", () => ({ default: () => null }));
vi.mock("../file-workspace/PdfReader", () => ({ default: () => null }));
vi.mock("../file-workspace/SourceEditor", async () => {
  const { forwardRef, useImperativeHandle } = await import("react");
  return {
    default: forwardRef(function SourceEditor(
      props: {
        label: string;
        value: string;
        path: string;
        reveal?: SourceLineRequest | null;
      },
      ref,
    ) {
      useImperativeHandle(ref, () => ({
        goToLine: mocks.goToLine,
        focus: vi.fn(),
      }));
      mocks.editorProps.push(props);
      return <textarea aria-label={props.label} value={props.value} readOnly />;
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
  mocks.listExecutionProfiles.mockResolvedValue([]);
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

it("suggests a first build from the open file and the files beside it", () => {
  expect(
    suggestBuild("paper/main.tex", [
      "paper/main.tex",
      "paper/refs.bib",
      "paper/main.pdf",
      "paper/fig/a.png",
      "notes.md",
    ]),
  ).toMatchObject({
    directory: "paper",
    rootDocument: "main.tex",
    expectedPdf: "main.pdf",
    inputs: ["main.tex", "refs.bib", "fig/a.png"],
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
