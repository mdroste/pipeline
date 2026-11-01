// Open files and their drafts. Unsaved text survives tab switches, restarts and
// eviction from memory; a clean file can always be read again.
import { useCallback, useEffect, useRef, useState } from "react";
import type { FileAdapter, FilePreview } from "../../lib/fileWorkspaceClient";
import { movedPath } from "../../lib/fileTree";
import { workbenchErrorMessage } from "../../lib/workbenchError";

export interface Buffer {
  file: FilePreview;
  draft: string;
  status: string;
  /** The last check found no file here. It stays watched until it returns. */
  missing?: boolean;
}
/** Clean files kept in memory beyond the ones on screen. */
const RETAINED_CLEAN = 12;

export const isDirty = (buffer: Buffer) =>
  buffer.file.editable && buffer.draft !== buffer.file.text;

const draftPrefix = (adapterId: string) => `pipeline.files.draft.${adapterId}.`;

/** Files with an unsaved draft. Read from where drafts are kept, because the
 * pages that need to know don't share these buffers. */
export function unsavedDraftPaths(adapterId: string) {
  const prefix = draftPrefix(adapterId);
  try {
    return Array.from({ length: localStorage.length }, (_, index) =>
      localStorage.key(index),
    ).flatMap((key) =>
      key?.startsWith(prefix) ? [key.slice(prefix.length)] : [],
    );
  } catch {
    return [];
  }
}

export function useFileBuffers({
  adapter,
  onSaved,
  onConflict,
  setError,
}: {
  adapter: FileAdapter;
  onSaved?: () => void;
  onConflict: (path: string) => void;
  setError: (message: string) => void;
}) {
  const [buffers, setBuffers] = useState<Record<string, Buffer>>({});
  const latest = useRef(buffers);
  latest.current = buffers;
  const [saving, setSaving] = useState<string | null>(null);
  const pending = useRef(new Map<string, Promise<void>>());
  const recent = useRef<string[]>([]);
  const alive = useRef(true);
  const adapterRef = useRef(adapter);
  adapterRef.current = adapter;
  const release = useCallback((file: FilePreview | undefined) => {
    if (file?.streamToken) adapterRef.current.release?.(file.streamToken);
  }, []);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      for (const buffer of Object.values(latest.current)) release(buffer.file);
    };
  }, [release]);
  const draftKey = useCallback(
    (path: string) => `${draftPrefix(adapter.id)}${path}`,
    [adapter.id],
  );
  const load = useCallback(
    async (path: string, reload: boolean | "discard" = false) => {
      recent.current = [...recent.current.filter((p) => p !== path), path];
      if (!reload && latest.current[path]) return;
      if (pending.current.has(path)) return pending.current.get(path);
      const before = latest.current[path];
      const operation = (async () => {
        const file = await adapter.read(path);
        let draft = file.text ?? "";
        let base = file;
        let status = "";
        if (!reload && file.editable) {
          try {
            const retained = JSON.parse(
              localStorage.getItem(draftKey(path)) ?? "null",
            );
            if (
              typeof retained?.draft === "string" &&
              typeof retained?.file?.hash === "string" &&
              typeof retained?.file?.text === "string" &&
              retained.file.path === path
            ) {
              draft = retained.draft;
              base = {
                ...file,
                hash: retained.file.hash,
                text: retained.file.text,
              };
              status =
                file.hash === base.hash
                  ? "Restored unsaved draft"
                  : "Restored draft; the file changed externally. Compare with the current file or reload before saving.";
            }
          } catch {
            /* Invalid draft ignored. */
          }
        }
        // Only a confirmed Reload may replace a draft, and never one that
        // changed during this read.
        const kept = (buffer: Buffer | undefined) =>
          Boolean(
            reload &&
            buffer &&
            (buffer !== before || (reload !== "discard" && isDirty(buffer))),
          );
        const edited = kept(latest.current[path]);
        release(edited ? file : before?.file);
        if (!alive.current) {
          if (!edited) release(file);
          return;
        }
        setBuffers((old) => ({
          ...old,
          [path]: kept(old[path])
            ? {
                ...old[path],
                status:
                  "The file was not reloaded because you made more edits. Your draft is still open.",
              }
            : { file: base, draft, status },
        }));
      })();
      pending.current.set(path, operation);
      try {
        await operation;
      } finally {
        pending.current.delete(path);
      }
    },
    [adapter, draftKey, release],
  );
  const retain = useCallback(
    (values: Record<string, Buffer>) => {
      for (const [path, buffer] of Object.entries(values)) {
        if (!buffer.file.editable) continue;
        try {
          if (buffer.draft !== buffer.file.text)
            localStorage.setItem(
              draftKey(path),
              JSON.stringify({
                file: { path, hash: buffer.file.hash, text: buffer.file.text },
                draft: buffer.draft,
              }),
            );
          else localStorage.removeItem(draftKey(path));
        } catch {
          if (alive.current)
            setError(
              "Draft storage is full. Save your files before closing Pipeline.",
            );
        }
      }
    },
    [draftKey, setError],
  );
  useEffect(() => {
    const timer = setTimeout(() => retain(buffers), 250);
    return () => clearTimeout(timer);
  }, [buffers, retain]);
  useEffect(() => {
    const flush = () => retain(latest.current);
    window.addEventListener("pagehide", flush);
    return () => {
      flush();
      window.removeEventListener("pagehide", flush);
    };
  }, [retain]);
  /** Forget clean files beyond the limit. Dirty ones keep their drafts. */
  const evict = useCallback(
    (onScreen: (string | null | undefined)[]) => {
      const keep = new Set(onScreen.filter(Boolean));
      const clean = recent.current.filter((path) => {
        const buffer = latest.current[path];
        return buffer && !keep.has(path) && !isDirty(buffer);
      });
      const excess = clean.slice(0, Math.max(0, clean.length - RETAINED_CLEAN));
      if (!excess.length) return;
      recent.current = recent.current.filter((p) => !excess.includes(p));
      for (const path of excess) release(latest.current[path]?.file);
      setBuffers((old) => {
        const next = { ...old };
        for (const path of excess) delete next[path];
        return next;
      });
    },
    [release],
  );
  const update = useCallback(
    (path: string, change: (buffer: Buffer) => Buffer) =>
      setBuffers((old) =>
        old[path] ? { ...old, [path]: change(old[path]) } : old,
      ),
    [],
  );
  const save = async (path: string, replacement?: string) => {
    const buffer = latest.current[path];
    if (!buffer || !adapter.save || !buffer.file.editable || saving) return;
    setSaving(path);
    setError("");
    const text = replacement ?? buffer.draft;
    try {
      const hash = await adapter.save(buffer.file, text);
      if (!alive.current) return;
      update(path, (old) => ({
        ...old,
        file: {
          ...old.file,
          hash,
          text,
          bytes: new TextEncoder().encode(text).length,
        },
        draft:
          replacement !== undefined && old.draft === buffer.draft
            ? text
            : old.draft,
        status: "Saved",
      }));
      onSaved?.();
    } catch (e) {
      if ((e as { code?: unknown } | null)?.code !== "storage_conflict") {
        setError(workbenchErrorMessage(e));
        return;
      }
      // A conflict means either a newer copy of the file or a save refused
      // while a response runs; the file's current hash tells them apart.
      let changed: boolean;
      try {
        const current = await adapter.read(path);
        release(current);
        changed = current.hash !== buffer.file.hash;
      } catch (readError) {
        if (alive.current) setError(workbenchErrorMessage(readError));
        return;
      }
      if (!alive.current) return;
      update(path, (old) => ({
        ...old,
        status: changed
          ? "The file changed elsewhere after you opened it. Compare with the current file to keep your draft, or reload to discard it."
          : "Stop the running response before saving.",
      }));
    } finally {
      setSaving(null);
    }
  };
  // Adopt the file's current bytes as the base and keep the draft, so Changes
  // shows the draft against the current file and Save can replace it.
  const compareWithCurrent = async (path: string) => {
    setError("");
    try {
      const current = await adapter.read(path);
      if (!alive.current) return release(current);
      release(latest.current[path]?.file);
      update(path, (old) => ({
        ...old,
        file: current,
        status:
          "Changes now compares your draft with the current file. Saving replaces the current file with your draft.",
      }));
      onConflict(path);
    } catch (e) {
      if (alive.current) setError(workbenchErrorMessage(e));
    }
  };
  /** Tabs not read since a restart hold their drafts only in storage. */
  const moveRetainedDrafts = useCallback(
    (from: string, to: string) => {
      const prefix = draftKey("");
      try {
        const keys = Array.from({ length: localStorage.length }, (_, index) =>
          localStorage.key(index),
        );
        for (const key of keys) {
          if (!key?.startsWith(prefix)) continue;
          const path = key.slice(prefix.length);
          const target = movedPath(path, from, to);
          if (!target) continue;
          let retained;
          try {
            retained = JSON.parse(localStorage.getItem(key) ?? "null");
          } catch {
            continue; // Invalid draft ignored.
          }
          // The prefix can also match an adapter whose id extends this one.
          if (retained?.file?.path !== path) continue;
          localStorage.setItem(
            draftKey(target),
            JSON.stringify({
              ...retained,
              file: { ...retained.file, path: target },
            }),
          );
          localStorage.removeItem(key);
        }
      } catch {
        setError(
          "Draft storage is full. Save your files before closing Pipeline.",
        );
      }
    },
    [draftKey, setError],
  );
  /** Carry open files, and their drafts, across a rename or move. */
  const remap = useCallback(
    (from: string, to: string) => {
      moveRetainedDrafts(from, to);
      const moved = Object.keys(latest.current).flatMap((path) => {
        const next = movedPath(path, from, to);
        return next ? [[path, next] as const] : [];
      });
      recent.current = recent.current.map((p) => movedPath(p, from, to) ?? p);
      for (const [path] of moved) {
        try {
          localStorage.removeItem(draftKey(path));
        } catch {
          /* The next retain rewrites the draft under its new path. */
        }
      }
      setBuffers((old) => {
        const next = { ...old };
        for (const [path, target] of moved) {
          if (!old[path]) continue;
          delete next[path];
          next[target] = {
            ...old[path],
            file: { ...old[path].file, path: target },
          };
        }
        return next;
      });
      return moved.map(([, target]) => target);
    },
    [draftKey, moveRetainedDrafts],
  );
  /** Drop a file that left the project, unless it holds unsaved text. */
  const forget = useCallback(
    (path: string) => {
      const buffer = latest.current[path];
      if (!buffer || isDirty(buffer)) return false;
      release(buffer.file);
      recent.current = recent.current.filter((p) => p !== path);
      setBuffers((old) => {
        const next = { ...old };
        delete next[path];
        return next;
      });
      return true;
    },
    [release],
  );
  return {
    buffers,
    latest,
    saving,
    load,
    retain,
    evict,
    update,
    save,
    compareWithCurrent,
    remap,
    forget,
    release,
  };
}
