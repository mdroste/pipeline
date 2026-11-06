import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { useReviewSetup } from "./useReviewSetup";
import { useReviewLaunch } from "./useReviewLaunch";
import type { PipelineConfig } from "../../lib/types";

const client = vi.hoisted(() => ({
  runSetup: vi.fn(),
  executionPlan: vi.fn(),
  pipelineConfig: vi.fn(),
  inputFiles: vi.fn(),
}));
vi.mock("../../lib/appClient", () => ({ appClient: client }));
vi.mock("../../lib/router", () => ({
  router: {
    route: { page: "main" },
    navigate: vi.fn(async () => true),
    apply: vi.fn(),
  },
}));
vi.mock("./rerunRequirements", () => ({ confirmRerunRequirements: vi.fn() }));

const queue = {
  enqueue: vi.fn(),
  enqueueBatch: vi.fn(),
  enqueueRerun: vi.fn(),
};
const navigation = { setPage: vi.fn(), clearHistorySourceSelection: vi.fn() };
const config = {
  steps: [],
  merge: { enabled: false, agents: [], prompt: "" },
} as unknown as PipelineConfig;
const selection = {
  paths: ["/paper.pdf"],
  interpretation: "document" as const,
  selectionKind: "file" as const,
};

beforeEach(() => {
  vi.clearAllMocks();
  client.runSetup.mockResolvedValue({
    profileId: "paper",
    profileConfigSnapshotId: "config",
    inputMode: "document",
    variables: [],
    inputSlots: [],
  });
  client.pipelineConfig.mockResolvedValue(config);
  client.executionPlan.mockResolvedValue({
    readiness: { ready: true, deps: [] },
    stages: [],
    parallelAgents: [],
    profileSnapshotId: "prepared",
    profileConfigSnapshotId: "config",
  });
  queue.enqueue.mockResolvedValue({ key: "queued" });
  queue.enqueueBatch.mockResolvedValue([
    { key: "batch-a" },
    { key: "batch-b" },
  ]);
});

async function setupReview() {
  const hook = renderHook(() => {
    const setup = useReviewSetup();
    return { setup, launch: useReviewLaunch(setup, queue, navigation) };
  });
  await act(async () => {
    await hook.result.current.setup.loadRunSetup();
  });
  act(() => {
    hook.result.current.setup.setPaperPath("/paper.pdf");
    hook.result.current.setup.setInputSelection(selection);
    hook.result.current.setup.setAuthorNote("  Model\r\n{var:literal}  ");
  });
  return hook;
}

it("freezes the normalized preview note even if the draft later changes", async () => {
  const { result } = await setupReview();
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  expect(client.executionPlan).toHaveBeenLastCalledWith(
    expect.objectContaining({ authorNote: "Model\n{var:literal}" }),
  );
  act(() => result.current.setup.setAuthorNote("Next draft"));
  await act(async () => {
    await result.current.launch.startPreparedLaunch();
  });
  expect(queue.enqueue).toHaveBeenCalledWith(
    expect.objectContaining({
      authorNote: "Model\n{var:literal}",
      expectedProfileSnapshotId: "prepared",
    }),
  );
});

it("rebuilds an edited preview while retaining entered workflow options", async () => {
  const { result } = await setupReview();
  client.runSetup.mockResolvedValue({
    profileId: "paper",
    profileConfigSnapshotId: "config",
    inputMode: "document",
    variables: [{ key: "audience" }],
    inputSlots: [],
  });
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  const pending = result.current.setup.pendingRun!;
  await act(async () => {
    await result.current.launch.launch(pending, { audience: "economists" });
  });
  act(() => result.current.launch.editPreviewAuthorNote());
  expect(result.current.launch.runPreview).toBeNull();
  expect(result.current.setup.authorNoteFocusKey).toBe(1);
  act(() => result.current.setup.setAuthorNote("Focus on assumptions"));
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  expect(client.executionPlan).toHaveBeenLastCalledWith(
    expect.objectContaining({
      authorNote: "Focus on assumptions",
      variables: { audience: "economists" },
    }),
  );
});

it("copies the note and preview fingerprint into a batch and resets for New review", async () => {
  const { result } = await setupReview();
  act(() =>
    result.current.setup.setInputSelection({
      paths: ["/a.pdf", "/b.pdf"],
      interpretation: "batch",
      selectionKind: "file",
    }),
  );
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  await act(async () => {
    await result.current.launch.startPreparedLaunch();
  });
  expect(queue.enqueueBatch).toHaveBeenCalledWith(
    expect.objectContaining({
      authorNote: "Model\n{var:literal}",
      paths: ["/a.pdf", "/b.pdf"],
      expectedProfileSnapshotId: "prepared",
    }),
  );
  await act(async () => {
    await result.current.launch.handleNewRun();
  });
  expect(result.current.setup.authorNote).toBe("");
});

it("retains a note after a queue failure, and blocks overlong drafts", async () => {
  const { result } = await setupReview();
  queue.enqueue.mockRejectedValueOnce(new Error("Queue unavailable"));
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  await act(async () => {
    await result.current.launch.startPreparedLaunch();
  });
  expect(result.current.setup.authorNote).toContain("Model");
  expect(result.current.launch.launchError).toContain("Queue unavailable");
  client.executionPlan.mockClear();
  act(() => result.current.setup.setAuthorNote("x".repeat(4001)));
  await act(async () => {
    await result.current.launch.handleGenerate();
  });
  expect(client.executionPlan).not.toHaveBeenCalled();
});

it("keeps a Workspace handoff's identity through review preview and queuing", async () => {
  const { result } = await setupReview();
  await act(async () => {
    await result.current.launch.handleWorkspaceReviewHandoff({
      version: 1,
      id: "review_handoff_a",
      workspaceId: "project",
      sessionId: null,
      paperId: "paper",
      revisionId: "revision",
      contentHash: "hash",
      mediaKind: "tex",
      stagedPath: "/workspace/handoffs/review_handoff_a/hash.tex",
      inputInterpretation: "document",
      metadata: {},
      externalReference: null,
      createdAt: "now",
      linkedAt: null,
    });
  });
  await act(async () => {
    await result.current.launch.startPreparedLaunch();
  });
  expect(queue.enqueue).toHaveBeenCalledWith(
    expect.objectContaining({
      paperPath: "/workspace/handoffs/review_handoff_a/hash.tex",
      reviewHandoffId: "review_handoff_a",
    }),
  );
});
