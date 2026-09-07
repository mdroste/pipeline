import { beforeEach, describe, expect, it, vi } from "vitest";
import { changedPaths, projectClient, textSelection, type Checkpoint, type DocumentView } from "./projectClient";
const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
beforeEach(() => invoke.mockReset());
describe("project contracts", () => {
  it("keeps host grant distinct from running or previewing a profile", async () => {
    invoke.mockResolvedValue({});
    await projectClient.previewExecution("profile");
    expect(invoke).toHaveBeenCalledExactlyOnceWith("workbench_preview_host_execution", { profileId: "profile" });
    await projectClient.authorizeExecution("profile", "reviewed-input-hash");
    expect(invoke).toHaveBeenLastCalledWith("workbench_authorize_host_execution", { profileId: "profile", fingerprint: "reviewed-input-hash" });
  });
  it("converts non-ASCII selections to exact byte locators", () => {
    const text = "α 😀 equation";
    const view = { start: 80, revision: { id: "old-revision", contentHash: "immutable-hash" } } as DocumentView;
    const selected = textSelection(text, 5, text.length, view);
    expect(selected).toMatchObject({ start: 88, end: 96, quote: "equation", revisionId: "old-revision", revisionHash: "immutable-hash" });
  });
  it("includes deletion, new files, and executable-bit changes in review", () => {
    const file = (path: string, hash: string, executable = false) => ({ path, hash, executable, size: 1, status: "captured" });
    const checkpoint = { base: { "gone.md": file("gone.md", "old"), "run.sh": file("run.sh", "same"), "same.md": file("same.md", "same") }, proposed: { "new.md": file("new.md", "new"), "run.sh": file("run.sh", "same", true), "same.md": file("same.md", "same") } } as unknown as Checkpoint;
    expect(changedPaths(checkpoint)).toEqual(["gone.md", "new.md", "run.sh"]);
  });
  it("sends the reviewed revision and only selected paths for acceptance", async () => {
    invoke.mockResolvedValue({ id: "application", body: {} });
    await projectClient.mutate("workspace", { action: "apply", checkpointId: "checkpoint", expectedRevision: 7, paths: ["paper.tex"] });
    expect(invoke).toHaveBeenCalledWith("workbench_project_mutate", { request: expect.objectContaining({ workspaceId: "workspace", operationId: expect.any(String), action: "apply", checkpointId: "checkpoint", expectedRevision: 7, paths: ["paper.tex"] }) });
  });
});
