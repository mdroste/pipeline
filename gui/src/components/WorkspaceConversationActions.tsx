// Per-conversation lifecycle controls for the Workspace sidebar: a small
// actions menu on each row, and the shared move dialog that the menu and the
// research panel's unfiled notice both open.

import { useEffect, useId, useRef, useState } from "react";
import type { WorkbenchSession, Workspace } from "../lib/workbenchTypes";

const menuItem =
  "block w-full rounded px-2 py-1.5 text-left text-sm hover:bg-gray-100 disabled:opacity-40 dark:hover:bg-neutral-800";

export function ConversationMenu({
  session,
  disabled,
  onRename,
  onGenerateTitle,
  onMove,
  onArchive,
  onDelete,
}: {
  session: WorkbenchSession;
  disabled: boolean;
  onRename: () => void;
  onGenerateTitle: () => void;
  onMove: () => void;
  onArchive: () => void;
  onDelete: () => void;
}) {
  const [open, setOpen] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onPointer = (event: MouseEvent) => {
      if (!container.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onPointer);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onPointer);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);
  const item = (label: string, action: () => void, danger = false) => (
    <button
      type="button"
      role="menuitem"
      onClick={() => {
        setOpen(false);
        action();
      }}
      className={`${menuItem}${danger ? " text-red-600 dark:text-red-400" : ""}`}
    >
      {label}
    </button>
  );
  return (
    <div ref={container} className="relative shrink-0">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Conversation actions for ${session.title}`}
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
        className="rounded px-2 py-1 text-gray-400 hover:bg-gray-200 hover:text-gray-700 disabled:opacity-40 dark:hover:bg-neutral-700 dark:hover:text-neutral-200"
      >
        ⋯
      </button>
      {open && (
        <div
          role="menu"
          aria-label={`Actions for ${session.title}`}
          className="absolute right-0 z-20 mt-1 w-44 rounded-lg border border-gray-200 bg-white p-1 shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
        >
          {item("Rename…", onRename)}
          {item("Generate title", onGenerateTitle)}
          {item("Move to project…", onMove)}
          {item(session.archivedAt ? "Restore" : "Archive", onArchive)}
          {item("Delete…", onDelete, true)}
        </div>
      )}
    </div>
  );
}

export function MoveConversationDialog({
  session,
  workspaces,
  busy,
  error,
  onMove,
  onClose,
}: {
  session: WorkbenchSession;
  workspaces: Workspace[];
  busy: boolean;
  error: string | null;
  onMove: (workspaceId: string | null) => void;
  onClose: () => void;
}) {
  const titleId = useId();
  const targets: Array<{ id: string | null; name: string }> = [
    ...(session.workspaceId ? [{ id: null, name: "Unfiled" }] : []),
    ...workspaces
      .filter(
        (workspace) =>
          !workspace.archivedAt && workspace.id !== session.workspaceId,
      )
      .map((workspace) => ({ id: workspace.id, name: workspace.name })),
  ];
  const [target, setTarget] = useState<string>(targets[0]?.id ?? "");
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [busy, onClose]);
  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/30 p-4">
      <form
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onSubmit={(event) => {
          event.preventDefault();
          if (targets.length) onMove(target || null);
        }}
        className="w-full max-w-md rounded-xl border border-gray-200 bg-white p-5 text-sm shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
      >
        <h2 id={titleId} className="text-sm font-semibold">
          Move “{session.title}”
        </h2>
        <p className="mt-2 text-xs leading-5 text-gray-500 dark:text-neutral-400">
          The conversation will use the new project’s settings and saved
          context. Its transcript stays here. Select a document again after
          moving.
        </p>
        {targets.length ? (
          <label className="mt-3 block text-xs font-medium">
            Destination
            <select
              aria-label="Destination project"
              value={target}
              onChange={(event) => setTarget(event.target.value)}
              disabled={busy}
              className="mt-1 w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm font-normal dark:border-neutral-700 dark:bg-neutral-950"
            >
              {targets.map((candidate) => (
                <option key={candidate.id ?? ""} value={candidate.id ?? ""}>
                  {candidate.name}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <p className="mt-3 text-xs text-gray-500">
            Create a project before moving this conversation.
          </p>
        )}
        {error && (
          <p
            role="alert"
            className="mt-3 text-xs text-red-600 dark:text-red-400"
          >
            {error}
          </p>
        )}
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            disabled={busy}
            onClick={onClose}
            className="rounded-md border border-gray-300 px-3 py-1.5 text-xs dark:border-neutral-700"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={busy || !targets.length}
            className="rounded-md bg-gray-900 px-3 py-1.5 text-xs font-medium text-white disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900"
          >
            {busy ? "Moving…" : "Move"}
          </button>
        </div>
      </form>
    </div>
  );
}
