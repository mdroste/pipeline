import type { FileHunk } from "./fileDiff";

/** One worker per mounted comparison, terminated on replacement or completion.
 * If workers cannot load, retain an exact whole-file edit without running an
 * expensive diff on the interaction thread. */
export function startFileDiff(before: string, after: string) {
  let worker: Worker | undefined;
  let rejectPending: ((error: Error) => void) | undefined;
  const result = new Promise<FileHunk[]>((resolve, reject) => {
    rejectPending = reject;
    const finish = (hunks: FileHunk[]) => {
      worker?.terminate();
      rejectPending = undefined;
      resolve(hunks);
    };
    const fallback = () =>
      finish(
        before === after
          ? []
          : [{ id: 0, from: 0, to: before.length, before, after, line: 1 }],
      );
    if (before === after) {
      finish([]);
      return;
    }
    try {
      worker = new Worker(new URL("./fileDiff.worker.ts", import.meta.url), {
        type: "module",
      });
      worker.onmessage = (event: MessageEvent<FileHunk[]>) =>
        finish(event.data);
      worker.onerror = (event) => {
        event.preventDefault();
        fallback();
      };
      worker.onmessageerror = fallback;
      worker.postMessage({ before, after });
    } catch {
      fallback();
    }
  });
  return {
    result,
    cancel: () => {
      worker?.terminate();
      rejectPending?.(new DOMException("Comparison replaced", "AbortError"));
      rejectPending = undefined;
    },
  };
}
