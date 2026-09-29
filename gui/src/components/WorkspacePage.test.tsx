import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { listen } from "@tauri-apps/api/event";
import WorkspacePage from "./WorkspacePage";
import type {
  ConversationSnapshot,
  WorkbenchSession,
  Workspace,
} from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  modelCatalog: vi.fn(),
  harnessCatalog: vi.fn(),
  listWorkspaces: vi.fn(),
  pendingRequests: vi.fn(),
  connectCodex: vi.fn(),
  accountState: vi.fn(),
  listSessions: vi.fn(),
  conversationSnapshot: vi.fn(),
  sessionSnapshot: vi.fn(),
  openConversationFile: vi.fn(),
  readConversationFile: vi.fn(),
  reconcileSession: vi.fn(),
  updateSession: vi.fn(),
  moveSession: vi.fn(),
  deleteSession: vi.fn(),
  generateSessionTitle: vi.fn(),
  sendTurn: vi.fn(),
  listPapers: vi.fn(),
  effectiveHarness: vi.fn(),
  importPaper: vi.fn(),
  open: vi.fn(),
}));
vi.mock("./WorkspaceProjectSurface", () => ({
  default: () => (
    <section aria-label="Project working area">Project content</section>
  ),
}));
vi.mock("../lib/programClient", () => ({
  programClient: { queue: vi.fn().mockResolvedValue([]) },
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
  save: vi.fn(),
}));
const workspace: Workspace = {
  id: "project",
  name: "Monetary policy",
  root: null,
  rootIdentity: null,
  settingsRevision: 1,
  revision: 1,
  archivedAt: null,
  missingRootAt: null,
  createdAt: "now",
  updatedAt: "now",
};
let snapshot: ConversationSnapshot;
const scrollIntoView = vi.fn();
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  const session: WorkbenchSession = {
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
  };
  snapshot = {
    workspace: null,
    session,
    sequence: 1,
    activeBinding: null,
    turns: [],
    items: [],
  };
  mocks.listWorkspaces.mockResolvedValue({ workspaces: [workspace] });
  mocks.pendingRequests.mockResolvedValue([]);
  mocks.modelCatalog.mockResolvedValue({
    models: [
      {
        id: "test-model",
        model: "test-model",
        displayName: "Test model",
        supportedReasoningEfforts: [
          { reasoningEffort: "high", description: "High" },
        ],
      },
    ],
  });
  mocks.connectCodex.mockResolvedValue({});
  mocks.accountState.mockResolvedValue({ status: "signedOut" });
  mocks.listSessions.mockImplementation(async () => ({
    sessions: [snapshot.session],
  }));
  mocks.conversationSnapshot.mockImplementation(async () => snapshot);
  mocks.sessionSnapshot.mockImplementation(async (id: string) =>
    mocks.conversationSnapshot(id),
  );
  mocks.openConversationFile.mockResolvedValue(undefined);
  mocks.readConversationFile.mockResolvedValue({
    path: "figure.png",
    hash: "figure-hash",
    bytes: 1,
    text: null,
    base64: "AA==",
    mime: "image/png",
    editable: false,
    truncated: false,
    externalPath: null,
  });
  mocks.reconcileSession.mockResolvedValue(false);
  mocks.updateSession.mockImplementation(async (request) => {
    snapshot = {
      ...snapshot,
      session: {
        ...snapshot.session,
        ...request,
        revision: snapshot.session.revision + 1,
      },
    };
    return { record: snapshot.session, sequence: 2 };
  });
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scrollIntoView,
  });
});
const mount = async () => {
  render(<WorkspacePage onOpenSettings={vi.fn()} />);
  await screen.findByRole("heading", { name: "Policy discussion" });
};

it("starts with navigation closed and preserves the draft while opening, pinning and closing it", async () => {
  await mount();
  const draft = screen.getByLabelText("Message");
  fireEvent.change(draft, { target: { value: "Keep my working question" } });
  expect(
    screen.queryByRole("complementary", { name: "Project navigation" }),
  ).not.toBeInTheDocument();
  const browse = screen.getByRole("button", { name: "Browse projects" });
  browse.focus();
  fireEvent.click(browse);
  expect(
    screen.getByRole("dialog", { name: "Browse projects" }),
  ).toBeInTheDocument();
  expect(draft.closest("[inert]")).not.toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Keep navigation open" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.getByRole("complementary", { name: "Project navigation" }),
  ).toBeInTheDocument();
  expect(localStorage.getItem("pipeline.workspace.navigationPinned")).toBe(
    "true",
  );
  expect(draft.closest("[inert]")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Close navigation" }));
  expect(localStorage.getItem("pipeline.workspace.navigationPinned")).toBe(
    "false",
  );
  expect(browse).toHaveFocus();
  expect(draft).toHaveValue("Keep my working question");
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("opens project tools with the keyboard while leaving a message draft untouched", async () => {
  localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
  localStorage.setItem("pipeline.workspace.sessionId", "conversation");
  localStorage.setItem(`pipeline.workspace.view.${workspace.id}`, "assistant");
  snapshot = {
    ...snapshot,
    workspace,
    session: { ...snapshot.session, workspaceId: workspace.id },
  };
  await mount();
  const draft = screen.getByLabelText("Message");
  fireEvent.change(draft, { target: { value: "Keep my assumptions" } });
  draft.focus();
  fireEvent.keyDown(draft, { key: "k", metaKey: true });
  const search = await screen.findByRole("combobox", {
    name: "Find a project view",
  });
  fireEvent.change(search, { target: { value: "execution settings" } });
  fireEvent.keyDown(search, { key: "Enter" });
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(
    screen.getByRole("button", { name: "Find a project view" }),
  ).toHaveTextContent("Execution settings");
  expect(draft).toHaveValue("Keep my assumptions");
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("explicit project entry reveals the project even when the saved view is conversation-only", async () => {
  localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
  localStorage.setItem("pipeline.workspace.sessionId", "conversation");
  localStorage.setItem(`pipeline.workspace.view.${workspace.id}`, "assistant");
  snapshot = {
    ...snapshot,
    workspace,
    session: { ...snapshot.session, workspaceId: workspace.id },
  };
  render(
    <WorkspacePage
      entrySurface="project"
      entryRequest={1}
      onOpenSettings={vi.fn()}
    />,
  );
  await waitFor(() =>
    expect(
      screen.getByRole("region", { name: "Project working area" }),
    ).toBeVisible(),
  );
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("saves the exact composer draft before allowing navigation back to the index", async () => {
  localStorage.setItem("pipeline.workspace.sessionId", "conversation");
  let save: (() => Promise<boolean>) | null = null;
  render(
    <WorkspacePage
      onSaveHandlerChange={(handler) => {
        save = handler;
      }}
      onOpenSettings={vi.fn()}
    />,
  );
  const message = await screen.findByLabelText("Message");
  await waitFor(() => expect(message).toBeEnabled());
  fireEvent.change(message, {
    target: { value: "Preserve this unfinished argument" },
  });
  let allowed = false;
  await act(async () => {
    allowed = await save!();
  });
  expect(allowed).toBe(true);
  expect(mocks.updateSession).toHaveBeenCalledWith(
    expect.objectContaining({
      sessionId: "conversation",
      draft: "Preserve this unfinished argument",
    }),
  );
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("makes absolute generated PDF and source paths clickable inside the project", async () => {
  localStorage.setItem("pipeline.workspace.workspaceId", workspace.id);
  localStorage.setItem("pipeline.workspace.sessionId", "conversation");
  localStorage.setItem(`pipeline.workspace.view.${workspace.id}`, "assistant");
  const rootedWorkspace = { ...workspace, root: "/Users/example/research" };
  snapshot = {
    ...snapshot,
    workspace: rootedWorkspace,
    session: { ...snapshot.session, workspaceId: workspace.id },
    items: [
      {
        id: "generated-files",
        turnId: null,
        providerItemId: "native-generated-files",
        itemKind: "agentMessage",
        payload: {
          text: "[Open PDF](/Users/example/research/output.pdf) · [Open source](/Users/example/research/source.tex#L8)",
        },
        isFinal: true,
        createdAt: "now",
        updatedAt: "now",
      },
    ],
  };
  const opened = vi.fn();
  window.addEventListener("pipeline:open-file", opened);

  await mount();

  const pdf = screen.getByRole("link", { name: "Open PDF" });
  expect(screen.getByRole("link", { name: "Open source" })).toHaveAttribute(
    "href",
    "/Users/example/research/source.tex#L8",
  );
  fireEvent.click(pdf);
  expect(opened).toHaveBeenCalledWith(
    expect.objectContaining({
      detail: {
        workspaceId: workspace.id,
        location: { path: "output.pdf" },
      },
    }),
  );
  window.removeEventListener("pipeline:open-file", opened);
});

it("renders generated images in an unfiled conversation", async () => {
  snapshot = {
    ...snapshot,
    items: [
      {
        id: "generated-image",
        turnId: null,
        providerItemId: "generated-image",
        itemKind: "agentMessage",
        payload: { text: "![Generated chart](figure.png)" },
        isFinal: true,
        createdAt: "now",
        updatedAt: "now",
      },
    ],
  };
  await mount();
  expect(
    await screen.findByRole("img", { name: "Generated chart" }),
  ).toHaveAttribute("src", "data:image/png;base64,AA==");
  expect(mocks.readConversationFile).toHaveBeenCalledWith(
    "conversation",
    "figure.png",
  );
});

it("flushes and clears the old conversation when a sidebar project event switches projects", async () => {
  await mount();
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Keep this draft before switching" },
  });

  act(() => {
    window.dispatchEvent(
      new CustomEvent("pipeline:open-project", { detail: workspace.id }),
    );
  });

  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        draft: "Keep this draft before switching",
      }),
    ),
  );
  await waitFor(() =>
    expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBe(
      workspace.id,
    ),
  );
  expect(
    screen.getByRole("region", { name: "Project working area" }),
  ).toBeVisible();
  expect(
    screen.queryByRole("heading", { name: "Policy discussion" }),
  ).not.toBeInTheDocument();
});

it("opens generated PDF and source links from an unfiled conversation", async () => {
  const root = "/private/workbench/jobs/conversations/conversation";
  snapshot.items = [
    {
      id: "scratch-files",
      turnId: null,
      providerItemId: "native-scratch-files",
      itemKind: "agentMessage",
      payload: {
        text: `[PDF](${root}/hello-world.pdf) · [LaTeX source](${root}/hello-world.tex)`,
      },
      isFinal: true,
      createdAt: "now",
      updatedAt: "now",
    },
  ];

  await mount();
  fireEvent.click(screen.getByRole("link", { name: "PDF" }));
  fireEvent.click(screen.getByRole("link", { name: "LaTeX source" }));

  expect(mocks.openConversationFile).toHaveBeenNthCalledWith(
    1,
    "conversation",
    `${root}/hello-world.pdf`,
  );
  expect(mocks.openConversationFile).toHaveBeenNthCalledWith(
    2,
    "conversation",
    `${root}/hello-world.tex`,
  );
});

it("restores the sidebar width, persists resizing, and resets without loading research tools", async () => {
  localStorage.setItem("pipeline.workspace.navigationPinned", "true");
  localStorage.setItem("pipeline.workspace.sidebarWidth", "336");
  await mount();
  const sidebar = screen.getByRole("complementary", {
    name: "Project navigation",
  });
  const divider = within(sidebar).getByRole("separator", {
    name: "Resize project sidebar",
  });
  expect(sidebar).toHaveStyle({ width: "336px" });
  fireEvent.keyDown(divider, { key: "ArrowRight" });
  expect(sidebar).toHaveStyle({ width: "344px" });
  expect(localStorage.getItem("pipeline.workspace.sidebarWidth")).toBe("344");
  fireEvent.doubleClick(divider);
  expect(sidebar).toHaveStyle({ width: "224px" });
  expect(localStorage.getItem("pipeline.workspace.sidebarWidth")).toBe("224");
  expect(mocks.listPapers).not.toHaveBeenCalled();
  expect(mocks.effectiveHarness).not.toHaveBeenCalled();
});

it("opens composer tools lazily and explains voice availability", async () => {
  await mount();
  expect(mocks.listPapers).not.toHaveBeenCalled();
  expect(mocks.effectiveHarness).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /Voice input/ }));
  expect(
    screen.getByText(
      "Live voice conversations are not available in Workspace yet.",
    ),
  ).toBeInTheDocument();
  fireEvent.click(
    screen.getByRole("button", { name: "Focus message for dictation" }),
  );
  expect(screen.getByRole("textbox", { name: "Message" })).toHaveFocus();
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("requires a project before importing and preserves a draft when switching", async () => {
  await mount();
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Keep this unsent question" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(screen.getByRole("button", { name: "Choose a project" }));
  fireEvent.click(screen.getByRole("button", { name: "Monetary policy" }));
  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        draft: "Keep this unsent question",
      }),
    ),
  );
  await waitFor(() =>
    expect(mocks.listSessions).toHaveBeenCalledWith("project", false),
  );
  expect(mocks.importPaper).not.toHaveBeenCalled();
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("jumps to an older response through search while keeping at most 200 transcript messages mounted", async () => {
  snapshot.items = Array.from({ length: 405 }, (_, index) => ({
    id: `message-${index}`,
    turnId: null,
    providerItemId: `native-${index}`,
    itemKind: index % 2 ? "agentMessage" : "userMessage",
    payload: {
      text:
        index === 1
          ? "The early identification argument"
          : `Message body ${index}`,
    },
    isFinal: true,
    createdAt: "now",
    updatedAt: "now",
  }));
  await mount();
  expect(screen.getAllByRole("article")).toHaveLength(200);
  expect(document.getElementById("workspace-message-message-1")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Conversation menu" }));
  fireEvent.click(screen.getByRole("button", { name: "Outline" }));
  fireEvent.change(screen.getByLabelText("Search prompts and responses"), {
    target: { value: "identification" },
  });
  const outline = within(
    screen.getByRole("complementary", { name: "Conversation outline" }),
  );
  fireEvent.click(
    outline.getByRole("button", {
      name: /Response.*The early identification argument/,
    }),
  );
  await waitFor(() =>
    expect(
      document.getElementById("workspace-message-message-1"),
    ).toHaveFocus(),
  );
  expect(screen.getAllByRole("article").length).toBeLessThanOrEqual(200);
  expect(scrollIntoView).toHaveBeenCalled();
  fireEvent.click(
    outline.getByRole("button", { name: "Close conversation outline" }),
  );
  fireEvent.click(screen.getByRole("button", { name: "↓ Latest" }));
  expect(
    document.getElementById("workspace-message-message-404"),
  ).toBeInTheDocument();
});

it("does not send Enter while an input method is composing", async () => {
  await mount();
  const input = screen.getByLabelText("Message");
  fireEvent.change(input, { target: { value: "研究" } });
  fireEvent.keyDown(input, { key: "Enter", isComposing: true });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("imports several files, reports failed extraction, and explicitly selects readable context", async () => {
  snapshot = {
    ...snapshot,
    workspace,
    session: {
      ...snapshot.session,
      workspaceId: workspace.id,
      draft: "My unsaved question",
    },
  };
  mocks.listPapers.mockResolvedValue([]);
  mocks.effectiveHarness.mockResolvedValue({ enabledModules: [] });
  mocks.open.mockResolvedValue(["/tmp/paper.md", "/tmp/scanned.pdf"]);
  mocks.importPaper.mockImplementation(async (request) => ({
    paper: { id: request.title, title: request.title },
    revision: {
      extraction: {
        status: request.title === "paper.md" ? "complete" : "failed",
        error: "No readable text",
      },
    },
  }));
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Choose files…" }));
  await screen.findByText("paper.md");
  await screen.findByText("Text unavailable: No readable text");
  expect(mocks.importPaper).toHaveBeenCalledTimes(2);
  const buttons = screen.getAllByRole("button", {
    name: "Use in conversation",
  });
  expect(buttons[0]).toBeDisabled();
  fireEvent.click(buttons[1]);
  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        paperId: "paper.md",
        presetId: "research_assistant",
      }),
    ),
  );
  expect(screen.getByLabelText("Message")).toHaveValue("My unsaved question");
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("blocks sending while files are being imported", async () => {
  snapshot = {
    ...snapshot,
    workspace,
    session: {
      ...snapshot.session,
      workspaceId: workspace.id,
      draft: "Read this",
    },
  };
  mocks.listPapers.mockResolvedValue([]);
  mocks.effectiveHarness.mockResolvedValue({
    enabledModules: ["paper_context"],
  });
  let release!: (value: null) => void;
  mocks.open.mockImplementation(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }),
  );
  await mount();
  fireEvent.click(screen.getByRole("button", { name: "Add files and tools" }));
  fireEvent.click(screen.getByRole("button", { name: /^Add filesDocuments/ }));
  fireEvent.click(await screen.findByRole("button", { name: "Choose files…" }));
  expect(screen.getByRole("button", { name: "Send ↑" })).toBeDisabled();
  expect(screen.getByLabelText("Project")).toBeDisabled();
  await act(async () => release(null));
  expect(screen.getByRole("button", { name: "Send ↑" })).toBeEnabled();
});

const openMenu = () => {
  if (!screen.queryByRole("complementary", { name: "Project navigation" }))
    fireEvent.click(screen.getByRole("button", { name: "Browse projects" }));
  fireEvent.click(
    screen.getByRole("button", {
      name: "Conversation actions for Policy discussion",
    }),
  );
};

it("renames a conversation from its row menu", async () => {
  await mount();
  vi.spyOn(window, "prompt").mockReturnValue("  Identification strategy  ");
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        title: "Identification strategy",
      }),
    ),
  );
  await screen.findByRole("heading", { name: "Identification strategy" });
  expect(screen.queryByRole("menu")).not.toBeInTheDocument();
});

it("deletes a conversation only after confirmation and clears the open transcript", async () => {
  await mount();
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  expect(mocks.deleteSession).not.toHaveBeenCalled();
  confirm.mockReturnValue(true);
  mocks.deleteSession.mockResolvedValue(7);
  mocks.listSessions.mockResolvedValue({ sessions: [] });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete…" }));
  await waitFor(() =>
    expect(mocks.deleteSession).toHaveBeenCalledWith(
      expect.objectContaining({ sessionId: "conversation" }),
    ),
  );
  await screen.findByText("No conversations yet.");
  expect(
    screen.queryByRole("heading", { name: "Policy discussion" }),
  ).not.toBeInTheDocument();
});

it("moves an unfiled conversation into a project and follows it there", async () => {
  await mount();
  mocks.moveSession.mockImplementation(async (request) => {
    snapshot = {
      ...snapshot,
      workspace,
      session: {
        ...snapshot.session,
        workspaceId: request.workspaceId,
        revision: snapshot.session.revision + 1,
      },
    };
    return { record: snapshot.session, sequence: 3 };
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  const dialog = screen.getByRole("dialog", {
    name: "Move “Policy discussion”",
  });
  expect(within(dialog).getByLabelText("Destination project")).toHaveValue(
    "project",
  );
  expect(
    within(dialog).queryByRole("option", { name: "Unfiled" }),
  ).not.toBeInTheDocument();
  fireEvent.click(within(dialog).getByRole("button", { name: "Move" }));
  await waitFor(() =>
    expect(mocks.moveSession).toHaveBeenCalledWith(
      expect.objectContaining({
        sessionId: "conversation",
        workspaceId: "project",
        expectedRevision: 1,
      }),
    ),
  );
  await waitFor(() =>
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
  );
  await waitFor(() =>
    expect(mocks.listSessions).toHaveBeenCalledWith("project", false),
  );
  expect(screen.getByLabelText("Project")).toHaveValue("project");
  expect(localStorage.getItem("pipeline.workspace.workspaceId")).toBe(
    "project",
  );
});

it("keeps a move dialog open with the error when the store refuses", async () => {
  await mount();
  mocks.moveSession.mockRejectedValue({
    code: "invalid_input",
    message:
      "This conversation has research records in its current Workspace and cannot be moved",
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Move to project…" }));
  fireEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", { name: "Move" }),
  );
  await waitFor(() =>
    expect(
      within(screen.getByRole("dialog")).getByRole("alert"),
    ).toHaveTextContent("cannot be moved"),
  );
  expect(screen.getByRole("dialog")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("regenerates a title on request and refreshes it from a background title event", async () => {
  await mount();
  mocks.generateSessionTitle.mockImplementation(async () => {
    snapshot = {
      ...snapshot,
      session: { ...snapshot.session, title: "Panel identification" },
    };
    return "Panel identification";
  });
  openMenu();
  fireEvent.click(screen.getByRole("menuitem", { name: "Generate title" }));
  await screen.findByRole("heading", { name: "Panel identification" });
  expect(mocks.generateSessionTitle).toHaveBeenCalledWith("conversation");
  const handler = vi
    .mocked(listen)
    .mock.calls.find(([name]) => name === "workbench:event")?.[1] as
    ((event: { payload: Record<string, unknown> }) => void) | undefined;
  expect(handler).toBeDefined();
  snapshot = {
    ...snapshot,
    session: { ...snapshot.session, title: "Automatic title" },
  };
  await act(async () => {
    handler!({
      payload: {
        kind: "sessionTitleUpdated",
        epoch: 1,
        sessionId: "conversation",
        title: "Automatic title",
      },
    });
  });
  await screen.findByRole("heading", { name: "Automatic title" });
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("recovers an immediate mode-exit draft even if its database save fails", async () => {
  const view = render(<WorkspacePage onOpenSettings={vi.fn()} />);
  await screen.findByRole("heading", { name: "Policy discussion" });
  mocks.updateSession.mockRejectedValue({ message: "Temporary store failure" });
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Exact unsent question αβ" },
  });
  view.unmount();
  await waitFor(() =>
    expect(mocks.updateSession).toHaveBeenCalledWith(
      expect.objectContaining({ draft: "Exact unsent question αβ" }),
    ),
  );
  expect(localStorage.getItem("pipeline.pendingDraft.conversation")).toBe(
    "Exact unsent question αβ",
  );
  render(<WorkspacePage onOpenSettings={vi.fn()} />);
  await screen.findByRole("heading", { name: "Policy discussion" });
  expect(screen.getByLabelText("Message")).toHaveValue(
    "Exact unsent question αβ",
  );
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("preserves the draft and blocks Send if the bottom model selector fails to save", async () => {
  mocks.accountState.mockResolvedValue({ status: "chatgpt" });
  await mount();
  await screen.findByRole("option", { name: "Test model" });
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Keep this question" },
  });
  mocks.updateSession.mockRejectedValue(new Error("Store unavailable"));
  fireEvent.change(screen.getByLabelText("Model"), {
    target: { value: "test-model" },
  });
  await screen.findByText(/Model selection could not be saved/);
  fireEvent.click(screen.getByRole("button", { name: "Send ↑" }));
  await act(async () => {});
  await screen.findByText(
    /Message not sent because its settings or draft could not be saved/,
  );
  expect(screen.getByLabelText("Message")).toHaveValue("Keep this question");
  expect(localStorage.getItem("pipeline.pendingDraft.conversation")).toBe(
    "Keep this question",
  );
  expect(mocks.sendTurn).not.toHaveBeenCalled();
});

it("waits for model selection to save before sending with the selected model", async () => {
  mocks.accountState.mockResolvedValue({ status: "chatgpt" });
  let release!: () => void;
  const saved = new Promise<void>((resolve) => {
    release = resolve;
  });
  const update = mocks.updateSession.getMockImplementation()!;
  mocks.updateSession.mockImplementation(async (request) => {
    if (request.overrides) await saved;
    return update(request);
  });
  mocks.sendTurn.mockResolvedValue({ threadId: "thread", turnId: "turn" });
  await mount();
  await screen.findByRole("option", { name: "Test model" });
  fireEvent.change(screen.getByLabelText("Model"), {
    target: { value: "test-model" },
  });
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Use the selected model" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Send ↑" }));
  expect(mocks.sendTurn).not.toHaveBeenCalled();
  await act(async () => release());
  await waitFor(() =>
    expect(mocks.sendTurn).toHaveBeenCalledWith(
      expect.objectContaining({
        model: "test-model",
        effort: null,
        text: "Use the selected model",
      }),
    ),
  );
});

it("retains unsaved assistant instructions and the message across inspector switches", async () => {
  const preset = {
    id: "research_assistant",
    name: "Research assistant",
    instructions: "Be precise.",
    modules: [],
    revision: 1,
    builtIn: true,
  };
  mocks.harnessCatalog.mockResolvedValue({ presets: [preset], modules: [] });
  mocks.effectiveHarness.mockResolvedValue({
    preset,
    mode: "inspect",
    contextBudgetBytes: 65536,
    enabledModules: [],
    unavailableModules: [],
    diagnostics: [],
    fingerprint: "test",
    valueSources: {},
  });
  await mount();
  fireEvent.change(screen.getByLabelText("Message"), {
    target: { value: "Draft message" },
  });
  fireEvent.click(
    within(
      screen.getByLabelText("Message").closest(".workspace-composer")!,
    ).getByRole("button", { name: "Assistant settings" }),
  );
  const instructions = await screen.findByLabelText(
    "Supplemental instructions",
  );
  fireEvent.click(
    screen.getByText("Create a customized profile", { selector: "summary" }),
  );
  fireEvent.change(instructions, {
    target: { value: "Keep these exact assumptions" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Conversation menu" }));
  fireEvent.click(screen.getByRole("button", { name: "Outline" }));
  expect(instructions).not.toBeVisible();
  fireEvent.click(
    within(
      screen.getByLabelText("Message").closest(".workspace-composer")!,
    ).getByRole("button", { name: "Assistant settings" }),
  );
  expect(instructions).toBeVisible();
  expect(instructions).toHaveValue("Keep these exact assumptions");
  expect(screen.getByLabelText("Message")).toHaveValue("Draft message");
  expect(
    screen.queryByRole("separator", { name: "Resize research workspace" }),
  ).not.toBeInTheDocument();
});
