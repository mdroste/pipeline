import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceAttachmentsPanel from "./WorkspaceAttachmentsPanel";
import { appEvents } from "../lib/appEvents";
import type { ConversationSnapshot } from "../lib/workbenchTypes";

const mocks = vi.hoisted(() => ({
  listPapers: vi.fn(),
  effectiveHarness: vi.fn(),
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const snapshot = {
  session: { id: "conversation", workspaceId: "project", paperId: null },
  activeBinding: null,
} as unknown as ConversationSnapshot;
const paper = (title: string, status: string) => ({
  paper: { id: title, title },
  revision: {
    id: `rev-${title}`,
    contentHash: `hash-${title}`,
    extraction: { status },
  },
});
const added: unknown[] = [];
let stop: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  added.length = 0;
  stop = appEvents.on("context-add", (detail) => added.push(detail));
  mocks.effectiveHarness.mockResolvedValue({
    enabledModules: ["paper_context"],
  });
  mocks.listPapers.mockResolvedValue([
    paper("Draft v8", "complete"),
    paper("Scanned appendix", "failed"),
  ]);
});
afterEach(() => stop());

it("lists project documents and attaches an exact version as a source", async () => {
  const close = vi.fn();
  render(
    <WorkspaceAttachmentsPanel
      mode="documents"
      snapshot={snapshot}
      onSnapshot={vi.fn()}
      onBusy={vi.fn()}
      onClose={close}
    />,
  );
  expect(
    screen.getByRole("status", { name: "Loading documents" }),
  ).toBeInTheDocument();
  await screen.findByText("Draft v8");
  expect(
    screen.queryByRole("button", { name: "Choose files…" }),
  ).not.toBeInTheDocument();
  const [readable, unreadable] = screen.getAllByRole("button", {
    name: "Add as source",
  });
  // A document whose text could not be read cannot be attached.
  expect(unreadable).toBeDisabled();
  fireEvent.click(readable);
  expect(added).toEqual([
    {
      workspaceId: "project",
      objects: [
        { kind: "paper", id: "rev-Draft v8", revision: "hash-Draft v8" },
      ],
    },
  ]);
  expect(close).toHaveBeenCalledOnce();
});

it("imports without first loading the whole document list", async () => {
  render(
    <WorkspaceAttachmentsPanel
      mode="import"
      snapshot={snapshot}
      onSnapshot={vi.fn()}
      onBusy={vi.fn()}
      onClose={vi.fn()}
    />,
  );
  expect(
    await screen.findByRole("button", { name: "Choose files…" }),
  ).toBeEnabled();
  expect(mocks.listPapers).not.toHaveBeenCalled();
});
