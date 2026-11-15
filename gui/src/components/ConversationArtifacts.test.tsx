import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { EditorView } from "@codemirror/view";
import { beforeEach, expect, it, vi } from "vitest";
import type {
  ConversationSnapshot,
  TranscriptItem,
} from "../lib/workbench/sessionTypes";
import ConversationArtifacts from "./ConversationArtifacts";

const mocks = vi.hoisted(() => ({
  listConversationFiles: vi.fn(),
  previewConversationFile: vi.fn(),
  saveConversationFile: vi.fn(),
  runArtifact: vi.fn(),
  artifactRunStatus: vi.fn(),
  confirmDialog: vi.fn(),
  openConversationFile: vi.fn(),
  openConversationFileWith: vi.fn(),
  transcriptPage: vi.fn(),
}));
vi.mock("../lib/workbench/conversationClient", () => ({
  conversationClient: mocks,
}));
vi.mock("./DialogService", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./DialogService")>()),
  confirmDialog: mocks.confirmDialog,
}));

function message(
  id: string,
  text: string,
  itemKind = "agentMessage",
): TranscriptItem {
  return {
    id,
    turnId: null,
    providerItemId: id,
    itemKind,
    payload: { text },
    isFinal: true,
    createdAt: "now",
    updatedAt: "now",
  };
}

const snapshot: ConversationSnapshot = {
  workspace: null,
  session: {
    id: "conversation",
    title: "Policy discussion",
    workspaceId: null,
    paperId: null,
    presetId: null,
    overrides: {},
    draft: "",
    revision: 1,
    archivedAt: null,
    createdAt: "now",
    updatedAt: "now",
  },
  sequence: 1,
  activeBinding: null,
  turns: [],
  items: [],
};

beforeEach(() => {
  vi.resetAllMocks();
  mocks.artifactRunStatus.mockResolvedValue(null);
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
});

it("opens a script from the private folder even when the assistant never linked it", async () => {
  mocks.listConversationFiles.mockResolvedValue({
    scopeRoot: "/private/conversation",
    listing: {
      directory: "",
      entries: [{ name: "estimate.do", path: "estimate.do", kind: "file" }],
      truncated: false,
    },
  });
  mocks.previewConversationFile.mockResolvedValue({
    path: "estimate.do",
    text: "summarize outcome",
    mime: "text/plain",
    scopeRoot: "/private/conversation",
    hash: "original",
    editable: true,
  });
  mocks.saveConversationFile.mockResolvedValue("saved");
  render(
    <ConversationArtifacts
      snapshot={snapshot}
      active={false}
      initial={null}
      onClose={vi.fn()}
    />,
  );
  fireEvent.click(await screen.findByRole("treeitem", { name: "estimate.do" }));
  expect(await screen.findByLabelText("Source language")).toHaveValue("stata");
  expect(mocks.previewConversationFile).toHaveBeenCalledWith(
    "conversation",
    "estimate.do",
  );
  fireEvent.click(screen.getByRole("button", { name: "More file actions" }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Open a copy with…" }));
  expect(mocks.openConversationFileWith).toHaveBeenCalledWith(
    "conversation",
    "estimate.do",
  );
  expect(mocks.openConversationFile).not.toHaveBeenCalled();
  const editor = EditorView.findFromDOM(
    screen.getByRole("textbox", { name: "Source editor: estimate.do" }),
  )!;
  expect(editor.state.readOnly).toBe(true);
  fireEvent.click(screen.getByRole("button", { name: "Edit" }));
  expect(editor.state.readOnly).toBe(false);
  act(() => {
    editor.dispatch({ changes: { from: 0, to: 9, insert: "describe" } });
  });
  expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByText("Saved")).toBeVisible();
  expect(mocks.saveConversationFile).toHaveBeenCalledWith({
    sessionId: "conversation",
    path: "estimate.do",
    scopeRoot: "/private/conversation",
    expectedHash: "original",
    content: "describe outcome",
  });
  fireEvent.click(screen.getByRole("button", { name: "Done editing" }));
  expect(editor.state.readOnly).toBe(true);
  const completed = {
    id: "run-1",
    state: "completed",
    stdout: "Stata batch output",
    stderr: "",
    exitStatus: 0,
    truncated: false,
  };
  mocks.confirmDialog.mockResolvedValue(true);
  mocks.runArtifact.mockResolvedValue({
    ...completed,
    state: "running",
    stdout: "",
  });
  mocks.artifactRunStatus.mockResolvedValue(completed);
  fireEvent.click(screen.getByRole("button", { name: "Run" }));
  expect(await screen.findByText("Stata batch output")).toBeVisible();
  expect(mocks.runArtifact).toHaveBeenCalledWith(
    {
      sessionId: "conversation",
      scopeRoot: "/private/conversation",
      path: "estimate.do",
    },
    "saved",
  );
});

it("finds assistant-linked files in the working copy and pages through earlier messages", async () => {
  const root = "/project/.pipeline-tasks/task";
  const before = { id: "recent", createdAt: "now" };
  mocks.listConversationFiles.mockResolvedValue({
    scopeRoot: root,
    listing: null,
  });
  mocks.transcriptPage.mockResolvedValue({
    items: [message("earlier", "[Old script](older.py)")],
    nextCursor: null,
  });
  mocks.previewConversationFile.mockResolvedValue({
    path: "results.md",
    text: "# Results",
    mime: "text/plain",
    editable: false,
  });
  render(
    <ConversationArtifacts
      snapshot={{
        ...snapshot,
        olderCursor: before,
        items: [
          message(
            "recent",
            `[Report][r]\n\n[r]: ${root}/results.md\n\n[Other](/elsewhere/outside.md)`,
          ),
          message("prompt", "[Input](input.txt)", "userMessage"),
        ],
      }}
      active={false}
      initial={null}
      onClose={vi.fn()}
    />,
  );
  const tree = await screen.findByRole("tree", { name: "Artifact files" });
  fireEvent.click(
    await within(tree).findByRole("treeitem", { name: "results.md" }),
  );
  expect(await screen.findByRole("heading", { name: "Results" })).toBeVisible();
  expect(within(tree).getAllByRole("treeitem")).toHaveLength(1);
  fireEvent.click(
    screen.getByRole("button", { name: "Find files in earlier messages" }),
  );
  expect(
    await within(tree).findByRole("treeitem", { name: "older.py" }),
  ).toBeVisible();
  expect(mocks.transcriptPage).toHaveBeenCalledWith("conversation", before);
});
