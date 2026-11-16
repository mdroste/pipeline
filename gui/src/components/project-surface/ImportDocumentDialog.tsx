import { useId, useRef, useState } from "react";
import useModalDialog from "../../hooks/useModalDialog";
import { researchClient } from "../../lib/workbench/researchClient";
import {
  documentRole,
  documentTypes,
  type DocumentType,
} from "../../lib/documentTypes";
import type { PaperWithRevision } from "../../lib/workbench/researchTypes";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { button, input, primaryButton, folderName, op } from "./shared";

export default function ImportDocumentDialog({
  workspaceId,
  path,
  folder,
  onClose,
  onImported,
}: {
  workspaceId: string;
  path: string;
  folder: boolean;
  onClose: () => void;
  onImported: (document: PaperWithRevision) => Promise<void>;
}) {
  const [title, setTitle] = useState(() =>
    folder ? folderName(path) : folderName(path).replace(/\.[^.]+$/, ""),
  );
  const [type, setType] = useState<DocumentType>("paper");
  const imported = useRef<PaperWithRevision | null>(null);
  const operationId = useRef(op());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const heading = useId();
  const dialog = useModalDialog<HTMLDivElement>(
    () => {
      if (!busy) onClose();
    },
    true,
    true,
  );
  const submit = async () => {
    if (busy || !title.trim()) return;
    setBusy(true);
    setError("");
    try {
      const document =
        imported.current ??
        (await researchClient.importPaper({
          workspaceId,
          paperId: null,
          title: title.trim(),
          role: documentRole(type),
          documentType: type,
          path,
          operationId: operationId.current,
        }));
      imported.current = document;
      await onImported(document);
      onClose();
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      ref={dialog}
      role="dialog"
      aria-modal="true"
      aria-labelledby={heading}
      tabIndex={-1}
      className="fixed inset-0 z-40 flex items-center justify-center bg-[var(--ui-scrim)] p-4"
    >
      <form
        className="ui-overlay-frame w-full max-w-md space-y-4 p-6"
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
      >
        <h2 id={heading} className="text-base font-semibold">
          Add document
        </h2>
        <p className="truncate text-xs text-ink-muted" title={path}>
          {folderName(path)}
        </p>
        <label className="block space-y-1 text-sm">
          Name
          <input
            className={input}
            data-autofocus
            value={title}
            maxLength={500}
            disabled={busy || Boolean(imported.current)}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="block space-y-1 text-sm">
          Document type
          <select
            className={input}
            value={type}
            disabled={busy || Boolean(imported.current)}
            onChange={(e) => setType(e.target.value as DocumentType)}
          >
            {documentTypes.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        {error && (
          <p role="alert" className="text-sm text-danger">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <button
            type="button"
            className={button}
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button className={primaryButton} disabled={busy || !title.trim()}>
            {busy ? "Adding…" : "Add document"}
          </button>
        </div>
      </form>
    </div>
  );
}
