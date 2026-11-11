import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { taskClient } from "../../lib/taskClient";
import type { FileListing } from "../../lib/fileWorkspaceClient";

export default function PaperInput({
  sessionId,
  value,
  onChange,
  onValidChange,
}: {
  sessionId: string;
  value: string;
  onChange: (path: string) => void;
  onValidChange: (valid: boolean) => void;
}) {
  const [browsing, setBrowsing] = useState(false);
  const [directory, setDirectory] = useState("");
  const [listing, setListing] = useState<FileListing | null>(null);
  const [file, setFile] = useState<{
    root: string;
    path: string;
    name: string;
  } | null>(null);
  const [error, setError] = useState("");
  const [listError, setListError] = useState("");
  const [attempt, setAttempt] = useState(0);
  const session = useRef(sessionId);
  session.current = sessionId;
  useEffect(() => {
    setDirectory("");
    setListing(null);
    setBrowsing(false);
  }, [sessionId]);
  useEffect(() => {
    let alive = true;
    setFile(null);
    setError("");
    onValidChange(!value);
    if (value && !sessionId) setError("Select a conversation first.");
    if (value && sessionId)
      void taskClient
        .validateInputFile(sessionId, value)
        .then((next) => {
          if (alive) {
            setFile(next);
            onValidChange(true);
          }
        })
        .catch((e) => {
          if (alive) setError(String(e));
        });
    return () => {
      alive = false;
    };
  }, [sessionId, value, attempt, onValidChange]);
  useEffect(() => {
    let alive = true;
    setListing(null);
    setListError("");
    if (browsing && sessionId)
      void taskClient
        .inputFiles(sessionId, directory)
        .then((next) => {
          if (alive) setListing(next);
        })
        .catch((e) => {
          if (alive) setListError(String(e));
        });
    return () => {
      alive = false;
    };
  }, [sessionId, directory, browsing, attempt]);
  async function browse() {
    const selectedSession = sessionId;
    try {
      const chosen = await open({
        multiple: false,
        directory: false,
        defaultPath: file?.root,
        title: "Choose a paper in the conversation folder",
      });
      if (typeof chosen === "string" && session.current === selectedSession)
        onChange(chosen);
    } catch (e) {
      if (session.current === selectedSession) setError(String(e));
    }
  }
  return (
    <div className="task-paper-input">
      <div className="task-actions">
        <button
          type="button"
          disabled={!sessionId}
          aria-expanded={browsing}
          onClick={() => setBrowsing(!browsing)}
        >
          Choose from project files
        </button>
        <button
          type="button"
          disabled={!sessionId}
          onClick={() => void browse()}
        >
          Browse…
        </button>
        {value && (
          <button type="button" onClick={() => onChange("")}>
            Clear file
          </button>
        )}
      </div>
      {!sessionId && (
        <p className="task-muted">
          Select a conversation to choose its input file.
        </p>
      )}
      {browsing && (
        <div className="task-file-picker" aria-label="Project files">
          <p>{directory || "Conversation folder"}</p>
          {directory && (
            <button
              type="button"
              onClick={() =>
                setDirectory(directory.split("/").slice(0, -1).join("/"))
              }
            >
              Up one folder
            </button>
          )}
          {!listing && !listError && <p role="status">Loading files…</p>}
          {listing && (
            <ul>
              {listing.entries
                .filter(
                  (entry) => entry.kind === "file" || entry.kind === "folder",
                )
                .map((entry) => (
                  <li key={entry.path}>
                    <button
                      type="button"
                      onClick={() => {
                        if (entry.kind === "folder") setDirectory(entry.path);
                        else {
                          onChange(entry.path);
                          setBrowsing(false);
                        }
                      }}
                    >
                      {entry.kind === "folder" ? `${entry.name}/` : entry.name}
                    </button>
                  </li>
                ))}
            </ul>
          )}
          {listing?.entries.length === 0 && <p>This folder is empty.</p>}
          {listing?.truncated && (
            <p>Some files are omitted. Use Browse to choose a file.</p>
          )}
          {listError && (
            <p role="alert">
              {listError}{" "}
              <button type="button" onClick={() => setAttempt((n) => n + 1)}>
                Retry files
              </button>
            </p>
          )}
        </div>
      )}
      {file && (
        <div role="status">
          <strong>{file.name}</strong>
          <p className="task-muted">
            {file.root} · {file.path}
          </p>
          <p>Revisions will be saved as Markdown.</p>
        </div>
      )}
      {value && !file && !error && <p role="status">Checking file…</p>}
      {error && (
        <p className="task-error" role="alert">
          {error}{" "}
          <button type="button" onClick={() => setAttempt((n) => n + 1)}>
            Check again
          </button>
        </p>
      )}
      <details>
        <summary>Enter a path (advanced)</summary>
        <label>
          File within the conversation’s folder
          <input
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder="papers/draft.pdf"
          />
        </label>
      </details>
    </div>
  );
}
