import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import Manuscript from "./Manuscript";
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
  fireEvent.click(screen.getByText("Load / refresh file"));
  await screen.findByLabelText("Manuscript source editor");
  expect(editorProps()).toMatchObject({
    path: "ws/working/main.tex",
    reveal: null,
  });
  await screen.findByRole("option", { name: "succeeded · execution" });
  fireEvent.change(screen.getByLabelText("Build"), {
    target: { value: "build_1" },
  });
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
  // An unsaved "Edit in" choice does not reopen the loaded file's editor.
  fireEvent.change(screen.getByLabelText("Edit in"), {
    target: { value: "checkpoint" },
  });
  expect(editorProps().path).toBe("ws/working/main.tex");
  fireEvent.click(screen.getByRole("button", { name: "main.tex:7" }));
  await waitFor(() => expect(editorProps().reveal).toEqual({ line: 7 }));
  expect(line).toHaveValue(7);
  expect(mocks.goToLine).toHaveBeenCalledOnce();
});
