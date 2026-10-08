import { act, render, screen, waitFor } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import useComposerFileDrop from "./useComposerFileDrop";
import { appEvents } from "../lib/appEvents";
import type { FileDropEvent } from "../lib/fileDrop";

const mocks = vi.hoisted(() => ({
  importPaper: vi.fn(),
  notify: vi.fn(),
  emit: null as ((event: FileDropEvent) => void) | null,
}));
vi.mock("../lib/workbenchClient", () => ({ workbenchClient: mocks }));
vi.mock("../components/DialogService", () => ({ notify: mocks.notify }));
vi.mock("../lib/fileDrop", () => ({
  onFileDrop: async (handler: (event: FileDropEvent) => void) => {
    mocks.emit = handler;
    return () => {
      mocks.emit = null;
    };
  },
}));

const onBusy = vi.fn();
const onError = vi.fn();
function Harness({
  workspaceId = "project" as string | null,
  blocked = false,
}) {
  const target = useRef<HTMLDivElement>(null);
  const state = useComposerFileDrop({
    targetRef: target,
    workspaceId,
    sessionId: "conversation",
    blocked,
    onBusy,
    onError,
  });
  return (
    <>
      <div ref={target} data-testid="pane" data-state={state} />
      <div data-testid="elsewhere" />
    </>
  );
}
const over = (testId: string) => {
  document.elementFromPoint = () => screen.getByTestId(testId);
};
const emit = (event: FileDropEvent) => act(async () => mocks.emit!(event));
const added: unknown[] = [];
let stop: () => void;
beforeEach(() => {
  vi.clearAllMocks();
  added.length = 0;
  stop = appEvents.on("context-add", (detail) => added.push(detail));
  mocks.importPaper.mockImplementation(async ({ title }) => ({
    paper: { id: title, title },
    revision: {
      id: `rev-${title}`,
      contentHash: `hash-${title}`,
      extraction: { status: title === "scan.pdf" ? "failed" : "complete" },
    },
  }));
});
afterEach(() => stop());

it("marks the conversation as a drop target only while files are over it", async () => {
  render(<Harness />);
  await waitFor(() => expect(mocks.emit).not.toBeNull());
  over("pane");
  await emit({ type: "over", x: 5, y: 5 });
  expect(screen.getByTestId("pane")).toHaveAttribute("data-state", "over");
  over("elsewhere");
  await emit({ type: "over", x: 500, y: 5 });
  expect(screen.getByTestId("pane")).toHaveAttribute("data-state", "idle");
  await emit({ type: "drop", x: 500, y: 5, paths: ["/tmp/paper.md"] });
  expect(mocks.importPaper).not.toHaveBeenCalled();
});

it("imports dropped files into the project and attaches the readable ones", async () => {
  render(<Harness />);
  await waitFor(() => expect(mocks.emit).not.toBeNull());
  over("pane");
  await emit({
    type: "drop",
    x: 5,
    y: 5,
    paths: ["/tmp/paper.md", "C:\\data\\scan.pdf"],
  });
  await waitFor(() => expect(onBusy).toHaveBeenLastCalledWith(false));
  expect(mocks.importPaper).toHaveBeenCalledTimes(2);
  expect(mocks.importPaper).toHaveBeenCalledWith(
    expect.objectContaining({
      workspaceId: "project",
      path: "C:\\data\\scan.pdf",
      title: "scan.pdf",
    }),
  );
  expect(onBusy).toHaveBeenNthCalledWith(1, true);
  expect(added).toEqual([
    {
      workspaceId: "project",
      whenReady: true,
      objects: [
        { kind: "paper", id: "rev-paper.md", revision: "hash-paper.md" },
      ],
    },
  ]);
  expect(mocks.notify).toHaveBeenCalledWith(
    "Added 1 file to this conversation.",
    "success",
  );
  expect(mocks.notify).toHaveBeenCalledWith(
    "1 file saved to the project, but the text could not be read.",
    "info",
  );
  expect(screen.getByTestId("pane")).toHaveAttribute("data-state", "idle");
});

it("reports files the project could not import", async () => {
  mocks.importPaper.mockRejectedValue(new Error("Unsupported file type"));
  render(<Harness />);
  await waitFor(() => expect(mocks.emit).not.toBeNull());
  over("pane");
  await emit({ type: "drop", x: 5, y: 5, paths: ["/tmp/movie.mov"] });
  await waitFor(() =>
    expect(onError).toHaveBeenCalledWith("movie.mov: Unsupported file type"),
  );
  expect(added).toEqual([]);
});

it("declines drops without a project or while a response is running", async () => {
  const view = render(<Harness workspaceId={null} />);
  await waitFor(() => expect(mocks.emit).not.toBeNull());
  over("pane");
  await emit({ type: "drop", x: 5, y: 5, paths: ["/tmp/paper.md"] });
  expect(mocks.notify).toHaveBeenLastCalledWith(
    expect.stringMatching(/saved in projects/),
    "info",
  );
  view.rerender(<Harness blocked />);
  await emit({ type: "drop", x: 5, y: 5, paths: ["/tmp/paper.md"] });
  expect(mocks.notify).toHaveBeenLastCalledWith(
    expect.stringMatching(/Wait for the current response/),
    "info",
  );
  expect(mocks.importPaper).not.toHaveBeenCalled();
  expect(onBusy).not.toHaveBeenCalled();
});
