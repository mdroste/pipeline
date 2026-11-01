import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import WorkspaceProjectSurface from "./WorkspaceProjectSurface";
import type ProjectDocuments from "./project-surface/ProjectDocuments";
import type ProjectSettings from "./project-surface/ProjectSettings";
import type { DocumentSelection } from "../lib/projectClient";
import { makeProjectHome } from "../test/projectSurfaceFixtures";

const mocks = vi.hoisted(() => ({
  home: vi.fn(),
  workspace: vi.fn(),
  mutate: vi.fn(),
  read: vi.fn(),
  note: vi.fn(),
  importPaper: vi.fn(),
  notify: vi.fn(),
  documents: vi.fn(),
  settings: vi.fn(),
}));
vi.mock("../lib/projectClient", async (original) => ({
  ...(await original<typeof import("../lib/projectClient")>()),
  projectClient: { home: mocks.home, mutate: mocks.mutate, read: mocks.read },
}));
vi.mock("../lib/workbench/workspaceClient", () => ({
  workspaceClient: { getWorkspace: mocks.workspace },
}));
vi.mock("../lib/workbench/researchClient", () => ({
  researchClient: { createNote: mocks.note, importPaper: mocks.importPaper },
}));
vi.mock("./DialogService", async (original) => ({
  ...(await original<typeof import("./DialogService")>()),
  notify: mocks.notify,
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
  onDestination?: (tab: string) => void,
) {
  render(
    <WorkspaceProjectSurface
      workspaceId="workspace"
      destination={destination}
      onDestination={onDestination}
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
it("sends a proposed revision to Review edits with its action item chosen", async () => {
  const onDestination = vi.fn();
  await mount("documents", onDestination);
  act(() => documentProps().onSelection(selected, "revision"));
  act(() => documentProps().onSubmitSelection());
  await waitFor(() => expect(onDestination).toHaveBeenCalledWith("edits"));
  expect(onDestination).not.toHaveBeenCalledWith("overview");
  expect(mocks.notify).not.toHaveBeenCalled();
});
it("keeps the reader open after adding an action item from a passage", async () => {
  const onDestination = vi.fn();
  await mount("documents", onDestination);
  act(() => documentProps().onSelection(selected, "task"));
  act(() => documentProps().setSelectionBody("Check this number"));
  act(() => documentProps().onSubmitSelection());
  await waitFor(() => expect(mocks.notify).toHaveBeenCalledOnce());
  await waitFor(() => expect(documentProps().selectionAction).toBeNull());
  expect(onDestination).not.toHaveBeenCalled();
  const [message, kind, link] = mocks.notify.mock.calls[0];
  expect(message).toBe("Action item added.");
  expect(kind).toBe("success");
  act(() => link.run());
  expect(onDestination).toHaveBeenCalledWith("tasks");
});
describe("adding a version", () => {
  const paper = {
    paper: { id: "paper", title: "Draft", role: "manuscript" },
    revision: { id: "latest", inputKind: "pdf" },
  };
  beforeEach(() => {
    mocks.home.mockResolvedValue({ ...makeProjectHome(), papers: [paper] });
    vi.mocked(open).mockResolvedValue("/research/new.pdf");
    mocks.importPaper.mockResolvedValue({ revision: { id: "newest" } });
  });
  it("finds the paper of an earlier version that is open", async () => {
    mocks.read.mockResolvedValue({
      revision: { id: "earlier", paperId: "paper" },
    });
    await mount();
    act(() => documentProps().setRevisionId("earlier"));
    await act(async () => documentProps().onAddVersion());
    await waitFor(() =>
      expect(mocks.importPaper).toHaveBeenCalledWith(
        expect.objectContaining({
          paperId: "paper",
          path: "/research/new.pdf",
        }),
      ),
    );
    await waitFor(() => expect(documentProps().revisionId).toBe("newest"));
  });
  it("says why when the open version has no paper left", async () => {
    mocks.read.mockResolvedValue({
      revision: { id: "earlier", paperId: "gone" },
    });
    await mount();
    act(() => documentProps().setRevisionId("earlier"));
    await act(async () => documentProps().onAddVersion());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "no longer in the project",
    );
    expect(open).not.toHaveBeenCalled();
  });
});
describe("the project error banner", () => {
  it("can be dismissed", async () => {
    await mount("project-settings");
    act(() => settingsProps().reportError("The save failed."));
    fireEvent.click(screen.getByRole("button", { name: "Dismiss this error" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });
  it("shows a structured rejection as text instead of crashing", async () => {
    await mount("project-settings");
    act(() => settingsProps().reportError({ code: "busy", message: "Busy" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Busy");
  });
  it("does not follow the researcher to another view", async () => {
    const surface = (destination: "documents" | "project-settings") => (
      <WorkspaceProjectSurface
        workspaceId="workspace"
        destination={destination}
        onConversation={vi.fn()}
        onWorkspaceChanged={vi.fn()}
      />
    );
    const page = render(surface("project-settings"));
    await screen.findByText("Settings fixture");
    act(() => settingsProps().reportError("The save failed."));
    page.rerender(surface("documents"));
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  });
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
async function focusRefresh() {
  vi.useFakeTimers();
  act(() => window.dispatchEvent(new Event("focus")));
  await act(async () => vi.advanceTimersByTime(400));
  vi.useRealTimers();
}
it("checks the folder on focus without disabling the project or clearing an error", async () => {
  await mount("project-settings");
  act(() => settingsProps().reportError("The earlier save failed."));
  let finish!: (value: unknown) => void;
  mocks.mutate.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  await focusRefresh();
  expect(mocks.mutate).toHaveBeenCalledWith("workspace", {
    action: "checkForChanges",
  });
  expect(document.querySelector("fieldset")).not.toBeDisabled();
  expect(screen.queryByRole("status")).toBeNull();
  await act(async () => finish({}));
  await waitFor(() => expect(mocks.home).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("alert")).toHaveTextContent(
    "The earlier save failed.",
  );
  mocks.mutate.mockRejectedValueOnce(new Error("Folder unavailable"));
  await focusRefresh();
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("alert")).toHaveTextContent(
    "The earlier save failed.",
  );
});
it("says nothing when a focus check finds the project in use", async () => {
  await mount("project-settings");
  mocks.mutate.mockRejectedValueOnce({
    code: "storage_conflict",
    message: "Another project operation is in progress; retry when it finishes",
  });
  await focusRefresh();
  await waitFor(() => expect(mocks.mutate).toHaveBeenCalledOnce());
  await act(async () => {});
  expect(screen.queryByRole("alert")).toBeNull();
});
it("starts a user action only after a focus check finishes", async () => {
  await mount("project-settings");
  let finish!: (value: unknown) => void;
  mocks.mutate.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  await focusRefresh();
  act(() => void settingsProps().saveSettings({ layout: "revision" }));
  expect(screen.getByRole("status")).toHaveTextContent("Saving…");
  expect(mocks.mutate).toHaveBeenCalledTimes(1);
  await act(async () => finish({}));
  await waitFor(() =>
    expect(mocks.mutate).toHaveBeenLastCalledWith(
      "workspace",
      expect.objectContaining({ action: "saveHome" }),
    ),
  );
  await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
});
