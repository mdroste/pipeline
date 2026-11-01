// Notice when open files change on disk. Polling, because researchers keep
// projects in sync folders and on network volumes, where system file events
// are least reliable. It stops while Files is hidden.
import { useEffect, useRef, type RefObject } from "react";
import type { FileAdapter } from "../../lib/fileWorkspaceClient";
import { isDirty, type Buffer } from "./useFileBuffers";

const INTERVAL_MS = 2000;
const MAX_PATHS = 64;

export default function useFileChanges({
  adapter,
  root,
  latest,
  load,
  update,
  release,
}: {
  adapter: FileAdapter;
  root: RefObject<HTMLElement | null>;
  latest: RefObject<Record<string, Buffer>>;
  load: (path: string, reload?: boolean) => Promise<void>;
  update: (path: string, change: (buffer: Buffer) => Buffer) => void;
  release: (file: Buffer["file"] | undefined) => void;
}) {
  const running = useRef(false);
  useEffect(() => {
    const stat = adapter.stat;
    if (!stat) return;
    const check = async () => {
      const element = root.current;
      if (
        running.current ||
        document.visibilityState === "hidden" ||
        !element ||
        element.closest("[hidden]")
      )
        return;
      const open = Object.values(latest.current).filter((b) => b.file.stamp);
      if (!open.length) return;
      running.current = true;
      try {
        const stamps = await stat(
          open.slice(0, MAX_PATHS).map((b) => b.file.path),
        );
        for (const { path, stamp } of stamps) {
          const buffer = latest.current[path];
          if (!buffer) continue;
          if (stamp === null) {
            if (!buffer.missing)
              update(path, (old) => ({
                ...old,
                missing: true,
                status: isDirty(old)
                  ? "This file was moved or deleted. Your draft is still open."
                  : "This file was moved or deleted.",
              }));
            continue;
          }
          // A file that returns can bring back the stamp it left with.
          if (buffer.file.stamp === stamp && !buffer.missing) continue;
          const current = await adapter.read(path);
          // Typing can land during that read, so decide from the buffer as it
          // is now. Whatever the disk holds, a draft stays.
          const now = latest.current[path];
          if (!now) {
            release(current);
            continue;
          }
          // Our own save, or a touch that left the bytes alone.
          if (current.hash === now.file.hash) {
            release(now.file);
            update(path, (old) => ({
              ...old,
              file: current,
              missing: false,
              status: old.missing ? "" : old.status,
            }));
          } else if (isDirty(now)) {
            release(current);
            update(path, (old) => ({
              ...old,
              missing: false,
              file: { ...old.file, stamp: current.stamp },
              status:
                "The file changed on disk. Compare with the current file to keep your draft, or reload to discard it.",
            }));
          } else {
            release(current);
            await load(path, true);
          }
        }
      } catch {
        /* The next check retries; saving still detects any conflict. */
      } finally {
        running.current = false;
      }
    };
    const timer = setInterval(() => void check(), INTERVAL_MS);
    const focus = () => void check();
    window.addEventListener("focus", focus);
    return () => {
      clearInterval(timer);
      window.removeEventListener("focus", focus);
    };
  }, [adapter, root, latest, load, update, release]);
}
