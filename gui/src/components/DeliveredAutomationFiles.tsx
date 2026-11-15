import { useEffect, useState } from "react";
import { taskClient, type DeliveredFile } from "../lib/taskClient";
import useModalDialog from "../hooks/useModalDialog";

export default function DeliveredAutomationFiles({
  sessionId,
  onClose,
}: {
  sessionId: string;
  onClose: () => void;
}) {
  const ref = useModalDialog<HTMLDivElement>(onClose, true, true);
  const [files, setFiles] = useState<DeliveredFile[]>([]);
  const [offset, setOffset] = useState(0);
  const [more, setMore] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let live = true;
    setLoading(true);
    taskClient
      .deliveredFiles(sessionId, offset)
      .then((page) => {
        if (live) {
          setFiles((prev) => (offset ? [...prev, ...page.files] : page.files));
          setMore(page.files.length === 50);
          if (page.warning) setError(page.warning);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [sessionId, offset]);
  async function save(file: DeliveredFile) {
    setSaving(true);
    try {
      await taskClient.saveDeliveredFile(sessionId, file);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }
  return (
    <div className="fixed inset-0 z-30 flex items-center justify-center bg-black/30 p-4">
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label="Files from automations"
        tabIndex={-1}
        className="max-h-[80vh] w-full max-w-lg overflow-auto rounded-lg border border-gray-200 bg-white p-4 shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
      >
        <h2 className="mb-3 text-sm font-semibold">Files from automations</h2>
        {error && <p role="alert">{error}</p>}
        {!files.length && !loading && !error && (
          <p>No files have been delivered to this conversation.</p>
        )}
        <ul className="space-y-2">
          {files.map((file) => (
            <li key={file.exchangeId + ":" + file.ordinal}>
              <button
                className="mr-2 underline"
                disabled={saving}
                onClick={() => void save(file)}
              >
                Save {file.filename}…
              </button>{" "}
              <small>{new Date(file.createdAt).toLocaleString()}</small>
            </li>
          ))}
        </ul>
        {loading && <p role="status">Loading files…</p>}
        {more && !loading && (
          <button onClick={() => setOffset(files.length)}>Show more</button>
        )}
        <button
          className="mt-3 rounded border px-2 py-1 text-sm"
          onClick={onClose}
        >
          Close
        </button>
      </div>
    </div>
  );
}
