/** Owns draft identity, persistence, external changes and save/discard races.
 * Build and PDF navigation call document intents; they never mutate draft refs. */
import { useState, useRef, useEffect } from "react";
import { manuscriptDraftStorage } from "../../lib/manuscriptDraftStorage";
import {
  readFileDraft,
  writeFileDraft,
  subscribeFileDrafts,
  projectFileScope,
} from "../../lib/sharedFileDrafts";
import { useDraftFlush } from "../../hooks/useDraftFlush";
import { studioClient } from "../../lib/studioClient";
import type { EditorFile } from "../../lib/studioTypes";
import { confirmDialog } from "../DialogService";
import type { StudioProps, useAction } from "./shared";
type Options = Pick<
  StudioProps,
  "workspaceId" | "documentId" | "active" | "onRefresh"
> &
  Pick<ReturnType<typeof useAction>, "run" | "setError"> & {
    reopened: number;
    onOpen: () => void;
    onCompare: () => void;
  };
export function useManuscriptDocument({
  workspaceId,
  documentId,
  active,
  onRefresh,
  reopened,
  run,
  setError,
  onOpen,
  onCompare,
}: Options) {
  const draftStorage = manuscriptDraftStorage(workspaceId, documentId);
  const [initialDraft] = useState(() => draftStorage.initial());
  const cached = useRef(initialDraft);
  const [path, setPath] = useState(
    cached.current?.file.path ?? draftStorage.location().path ?? "",
  );
  const [checkpoint, setCheckpoint] = useState(
    cached.current?.file.checkpointId ??
      draftStorage.location().checkpointId ??
      "",
  );
  const [file, setFile] = useState<EditorFile | null>(null);
  const [draft, setDraft] = useState("");
  const [recoveredDraft, setRecoveredDraft] = useState<string | null>(null);
  const draftOwner = useRef({});
  // How the file on disk relates to the text the open draft started from.
  const [fileState, setFileState] = useState<
    "current" | "changed" | "compared"
  >("current");
  const [notice, setNotice] = useState(
    cached.current
      ? "An unsaved draft is retained. Load its file to restore it."
      : "",
  );
  const dirty = file !== null && draft !== file.content;
  const draftState = useRef({ file, draft });
  draftState.current = { file, draft };
  const share = (opened: EditorFile, text: string) =>
    writeFileDraft(
      projectFileScope(workspaceId, opened.checkpointId),
      {
        file: { path: opened.path, hash: opened.hash, text: opened.content },
        draft: text,
      },
      draftOwner.current,
    );
  useEffect(
    () =>
      subscribeFileDrafts(
        projectFileScope(workspaceId, checkpoint),
        (value, source) => {
          const current = draftState.current.file;
          if (
            !current ||
            source === draftOwner.current ||
            current.path !== value.file.path
          )
            return;
          const next = {
            file: {
              ...current,
              hash: value.file.hash,
              content: value.file.text,
            },
            draft: value.draft,
          };
          draftState.current = next;
          setFile(next.file);
          setDraft(next.draft);
        },
      ),
    [workspaceId, checkpoint],
  );
  const editDraft = (text: string) => {
    const current = draftState.current.file;
    draftState.current = { file: current, draft: text };
    setDraft(text);
    if (current)
      try {
        share(current, text);
      } catch {
        setError("Draft storage is full. Save before closing this editor.");
      }
  };
  const persistDraft = () => {
    const current = draftState.current;
    try {
      if (current.file) {
        share(current.file, current.draft);
        draftStorage.saveLocation(current.file);
      }
    } catch {
      if (!current.file || current.draft === current.file.content) return true;
      setError(
        "Draft storage is full. Keep this editor open until the file is saved.",
      );
      return false;
    }
    return true;
  };
  useEffect(() => {
    const timer = setTimeout(persistDraft, 300);
    return () => clearTimeout(timer);
  }, [file, draft]);
  useDraftFlush(persistDraft);
  /** Keep the open draft so it returns when this file is opened again. */
  const stashDraft = () => {
    if (!persistDraft())
      throw new Error("Save your draft before opening another file.");
  };
  const openFile = async (target: string, inCopy: string) => {
    onOpen();
    const next = await studioClient.editor(workspaceId, target, inCopy || null);
    const legacy = draftStorage.readFile(inCopy, target) ?? cached.current;
    const shared = readFileDraft(
      projectFileScope(workspaceId, inCopy),
      next.path,
    );
    const retained = shared
      ? {
          file: { ...next, hash: shared.file.hash, content: shared.file.text },
          draft: shared.draft,
        }
      : legacy;
    setRecoveredDraft(
      shared &&
        legacy?.file.path === next.path &&
        legacy.file.checkpointId === next.checkpointId &&
        legacy.draft !== shared.draft
        ? legacy.draft
        : null,
    );
    setPath(next.path);
    setCheckpoint(next.checkpointId ?? "");
    if (
      retained?.file.path === next.path &&
      retained.file.checkpointId === next.checkpointId
    ) {
      setFile(retained.file);
      setDraft(retained.draft);
      setFileState(retained.file.hash === next.hash ? "current" : "changed");
      setNotice("Restored unsaved draft");
    } else {
      setFile(next);
      setDraft(next.content);
      setFileState("current");
      setNotice("");
    }
    const restored =
      retained?.file.path === next.path &&
      retained.file.checkpointId === next.checkpointId
        ? retained
        : { file: next, draft: next.content };
    draftState.current = restored;
    share(restored.file, restored.draft);
    // Migrate a matching legacy draft once; a differing legacy copy stays recoverable.
    if (legacy?.draft === restored.draft) {
      draftStorage.forget(next);
    }
  };
  const forgetDraft = () => {
    cached.current = null;
    try {
      draftStorage.forget(file);
    } catch {
      /* Current draft is still discarded. */
    }
  };
  // A confirmation applies to the text and file the user saw. Never clear
  // retained text until the replacement has arrived and both still match.
  const discard = async (reload = false) => {
    const { file: opened, draft: confirmed } = draftState.current;
    if (
      !opened ||
      !(await confirmDialog(
        reload
          ? `Reload ${opened.path} from the project and discard your unsaved draft?`
          : `Discard your unsaved draft of ${opened.path}?`,
        {
          title: "Discard unsaved draft",
          confirmLabel: reload ? "Discard and reload" : "Discard",
          destructive: true,
        },
      ))
    )
      return;
    await run(async () => {
      const unchanged = () =>
        draftState.current.file === opened &&
        draftState.current.draft === confirmed;
      if (!unchanged()) return;
      const current = await studioClient.editor(
        workspaceId,
        opened.path,
        opened.checkpointId,
      );
      if (!unchanged()) return;
      forgetDraft();
      setFile(current);
      setDraft(current.content);
      draftState.current = { file: current, draft: current.content };
      share(current, current.content);
      setFileState("current");
      setNotice(reload ? "Reloaded from the project" : "Draft discarded");
    });
  };
  const load = () =>
    dirty
      ? discard(true)
      : run(async () => {
          await openFile(path, checkpoint);
          setNotice((old) => old || "Reloaded from the project");
        });
  const switchFile = (target: string, inCopy = checkpoint) =>
    run(async () => {
      stashDraft();
      await openFile(target, inCopy);
    });
  /** Notice a file that changed while this retained editor kept its text. A
   * clean editor follows the file; a draft stays, and Compare can adopt it. */
  const checkFile = async () => {
    const opened = draftState.current.file;
    if (!opened) return;
    const current = await studioClient
      .editor(workspaceId, opened.path, opened.checkpointId)
      .catch(() => null);
    // Typing can land during that read, and a save or another file replaces
    // the record, so decide from the editor as it is now.
    const now = draftState.current;
    if (!current || now.file !== opened) return;
    if (current.hash === opened.hash) {
      setFileState((old) => (old === "changed" ? "current" : old));
    } else if (now.draft !== opened.content) {
      setFileState("changed");
    } else {
      setFile(current);
      setDraft(current.content);
      setFileState("current");
      setNotice("Updated from the project");
    }
  };
  const latestCheck = useRef(checkFile);
  latestCheck.current = checkFile;
  useEffect(() => {
    if (active === false) return;
    if (reopened) void latestCheck.current();
    const focus = () => void latestCheck.current();
    window.addEventListener("focus", focus);
    return () => window.removeEventListener("focus", focus);
  }, [active, reopened]);
  /** Adopt the current file as the base, so Changes shows the draft against it
   * and Save can replace it. */
  const compare = () =>
    run(async () => {
      if (!file) return;
      setFile(
        await studioClient.editor(workspaceId, file.path, file.checkpointId),
      );
      setFileState("compared");
      onCompare();
    });
  const saveCurrent = async () => {
    if (!file) return true;
    let response: { hash: string };
    try {
      response = await studioClient.mutate(workspaceId, {
        action: "saveText",
        checkpointId: file.checkpointId,
        path: file.path,
        expectedHash: file.hash,
        content: draft,
      });
    } catch (e) {
      // A conflict is a newer file, or a save refused while a response runs.
      if ((e as { code?: unknown } | null)?.code !== "storage_conflict")
        throw e;
      const current = await studioClient.editor(
        workspaceId,
        file.path,
        file.checkpointId,
      );
      if (current.hash === file.hash) throw e;
      setFileState("changed");
      return false;
    }
    const current = draftState.current;
    const saved = { ...file, content: draft, hash: response.hash };
    if (
      current.file?.path === file.path &&
      current.file.checkpointId === file.checkpointId
    ) {
      setFile(saved);
      draftState.current = { file: saved, draft: current.draft };
      share(saved, current.draft);
    }
    setFileState("current");
    if (recoveredDraft === null) forgetDraft();
    setNotice("Saved. Rebuild the manuscript to check the updated files.");
    await onRefresh();
    return true;
  };
  const save = () => run(saveCurrent);
  return {
    path,
    setPath,
    checkpoint,
    setCheckpoint,
    file,
    draft,
    recoveredDraft,
    setRecoveredDraft,
    fileState,
    notice,
    setNotice,
    dirty,
    editDraft,
    stashDraft,
    openFile,
    forgetDraft,
    discard,
    load,
    switchFile,
    compare,
    saveCurrent,
    save,
    hasRetainedDraft: Boolean(cached.current),
  };
}
