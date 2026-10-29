import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import type ProjectDocuments from "./project-surface/ProjectDocuments";
import type ProjectSettings from "./project-surface/ProjectSettings";
import type { DocumentSelection } from "../lib/projectClient";
import { makeProjectHome } from "../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  workspace: vi.fn(),
  mutate: vi.fn(),
  note: vi.fn(),
  documents: vi.fn(),
  settings: vi.fn(),
}));
vi.mock("../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../lib/projectClient")>()),
  projectClient: { home: mocks.home, mutate: mocks.mutate },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.workspace },
}));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { createNote: mocks.note },
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./project-surface/ProjectDocuments", () => ({
  default: (props: ComponentProps<typeof ProjectDocuments>) => {
    mocks.documents(props);
    return <div>Document fixture</div>;
  },
}));
vi.mock("./project-surface/ProjectSettings", () => ({
  default: (props: ComponentProps<typeof ProjectSettings>) => {
    mocks.settings(props);
    return <div>Settings fixture</div>;
  },
}));
const documentProps = () =>
  mocks.documents.mock.lastCall![0] as ComponentProps<typeof ProjectDocuments>;
const settingsProps = () =>
  mocks.settings.mock.lastCall![0] as ComponentProps<typeof ProjectSettings>;
const selected: DocumentSelection = {
  revisionId: "revision",
  revisionHash: "immutable-hash",
  start: 11,
  end: 29,
  page: null,
  region: null,
  quote: "Selected evidence",
};
beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
  vi.clearAllMocks();
  mocks.home.mockResolvedValue(makeProjectHome());
  mocks.workspace.mockResolvedValue({
    id: "workspace",
    root: "/research",
    name: "Local research",
    revision: 1,
  });
  mocks.mutate.mockResolvedValue({ id: "annotation", revision: 1, body: {} });
  mocks.note.mockResolvedValue({});
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
async function mount(
  destination: "documents" | "project-settings" = "documents",
) {
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      destination={destination}
      onConversation={vi.fn()}
      onWorkspaceChanged={vi.fn()}
    />,
  );
  await screen.findByText(
    destination === "documents" ? "Document fixture" : "Settings fixture",
  );
}
it("retains the mutation gate until the post-mutation refresh finishes", async () => {
  await mount();
  let finishMutation!: (value: unknown) => void;
  let finishRefresh!: (value: unknown) => void;
  mocks.mutate.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishMutation = resolve;
      }),
  );
  mocks.home.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finishRefresh = resolve;
      }),
  );
  let operation!: Promise<unknown>;
  act(() => {
    operation = documentProps().act({ action: "refresh" });
    void documentProps().act({ action: "refresh" });
  });
  expect(mocks.mutate).toHaveBeenCalledTimes(1);
  await act(async () => finishMutation({}));
  expect(mocks.home).toHaveBeenCalledTimes(2);
  await act(async () => {
    await documentProps().act({ action: "refresh" });
  });
  expect(mocks.mutate).toHaveBeenCalledTimes(1);
  await act(async () => {
    finishRefresh(makeProjectHome());
    await operation;
  });
  await act(async () => {
    await documentProps().act({ action: "refresh" });
  });
  expect(mocks.mutate).toHaveBeenCalledTimes(2);
});
it("creates an immutable annotation before saving its accepted note", async () => {
  await mount();
  let finish!: (value: unknown) => void;
  mocks.mutate.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  act(() => documentProps().onSelection(selected, "note"));
  act(() => documentProps().setSelectionBody("Keep this passage"));
  act(() => documentProps().onSubmitSelection());
  expect(mocks.mutate).toHaveBeenCalledWith("workspace", {
    action: "annotate",
    selection: selected,
    body: "Keep this passage",
  });
  expect(mocks.note).not.toHaveBeenCalled();
  await act(async () => finish({ id: "anchor-id", revision: 1, body: {} }));
  await waitFor(() =>
    expect(mocks.note).toHaveBeenCalledWith({
      workspaceId: "workspace",
      paperId: null,
      kind: "decision",
      body: "Keep this passage\n\nSource annotation: anchor-id; revision revision (immutable-hash); bytes 11–29.",
      state: "accepted",
      origin: "user",
      pinned: false,
      operationId: expect.any(String),
    }),
  );
  expect(documentProps().selectionAction).toBeNull();
});
it("anchors a revision action before creating the task with that anchor", async () => {
  await mount();
  act(() => documentProps().onSelection(selected, "revision"));
  expect(documentProps().selectionBody).toBe(
    "Revise this passage, preserving substantive claims and equations.",
  );
  act(() => documentProps().onSubmitSelection());
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalledTimes(2));
  expect(mocks.mutate.mock.calls[0]).toEqual([
    "workspace",
    {
      action: "annotate",
      selection: selected,
      body: "Revise this passage, preserving substantive claims and equations.",
    },
  ]);
  expect(mocks.mutate.mock.calls[1]).toEqual([
    "workspace",
    {
      action: "createTask",
      objective:
        "Revise this passage, preserving substantive claims and equations.",
      anchorId: "annotation",
      expectedOutputs: [],
      expectedChecks: [],
    },
  ]);
});
it("debounces focus refreshes for 400ms and watches only at the existing 30-second interval", async () => {
  await mount("project-settings");
  vi.useFakeTimers();
  act(() => window.dispatchEvent(new Event("focus")));
  act(() => vi.advanceTimersByTime(200));
  act(() => window.dispatchEvent(new Event("focus")));
  act(() => vi.advanceTimersByTime(399));
  expect(mocks.mutate).not.toHaveBeenCalled();
  await act(async () => vi.advanceTimersByTime(1));
  expect(mocks.mutate).toHaveBeenCalledTimes(1);
  act(() => settingsProps().setWatch(true));
  await act(async () => vi.advanceTimersByTime(30_000));
  expect(mocks.mutate).toHaveBeenCalledTimes(1);
  await act(async () => vi.advanceTimersByTime(400));
  expect(mocks.mutate).toHaveBeenCalledTimes(2);
});
