import { afterEach, describe, expect, it, vi } from "vitest";
import { startFileDiff } from "./fileDiffClient";

class TestWorker {
  static instances: TestWorker[] = [];
  onmessage?: (event: { data: unknown }) => void;
  onerror?: (event: { preventDefault: () => void }) => void;
  postMessage = vi.fn();
  terminate = vi.fn();
  constructor() {
    TestWorker.instances.push(this);
  }
}
afterEach(() => {
  vi.unstubAllGlobals();
  TestWorker.instances = [];
});
describe("file comparison worker", () => {
  it("delivers results and releases the worker", async () => {
    vi.stubGlobal("Worker", TestWorker);
    const job = startFileDiff("old", "new");
    const worker = TestWorker.instances[0];
    expect(worker.postMessage).toHaveBeenCalledWith({
      before: "old",
      after: "new",
    });
    worker.onmessage?.({
      data: [{ id: 0, from: 0, to: 3, before: "old", after: "new", line: 1 }],
    });
    expect(await job.result).toHaveLength(1);
    expect(worker.terminate).toHaveBeenCalledOnce();
  });
  it("terminates replaced comparisons and rejects their pending result", async () => {
    vi.stubGlobal("Worker", TestWorker);
    const job = startFileDiff("old", "new");
    const result = expect(job.result).rejects.toMatchObject({
      name: "AbortError",
    });
    job.cancel();
    await result;
    expect(TestWorker.instances[0].terminate).toHaveBeenCalledOnce();
  });
  it("falls back to an exact whole-file change when a worker fails", async () => {
    vi.stubGlobal("Worker", TestWorker);
    const job = startFileDiff("old\r\n", "new\n😀");
    TestWorker.instances[0].onerror?.({ preventDefault: vi.fn() });
    expect(await job.result).toEqual([
      { id: 0, from: 0, to: 5, before: "old\r\n", after: "new\n😀", line: 1 },
    ]);
    expect(TestWorker.instances[0].terminate).toHaveBeenCalledOnce();
  });
  it("does not allocate a worker for identical inputs", async () => {
    vi.stubGlobal("Worker", TestWorker);
    expect(await startFileDiff("same", "same").result).toEqual([]);
    expect(TestWorker.instances).toHaveLength(0);
  });
});
