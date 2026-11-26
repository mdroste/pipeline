import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { appEvents } from "../../lib/appEvents";
import {
  projectIntegrations,
  type CitationIndex,
  type ZoteroPreview,
} from "../../lib/projectIntegrations";
import { ErrorNotice, Field, useAction } from "./shared";

function ZoteroImport({ workspaceId }: { workspaceId: string }) {
  const [preview, setPreview] = useState<ZoteroPreview | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [notice, setNotice] = useState("");
  const { run, busy, error } = useAction();
  const load = (collection: string | null, start = 0) =>
    void run(async () => {
      setPreview(
        await projectIntegrations.zotero(
          collection,
          start,
          collection ? preview?.serverId : null,
        ),
      );
      setSelected([]);
      setNotice("");
    });
  return (
    <details>
      <summary>Import selected Zotero metadata</summary>
      <p className="text-xs">
        Open Zotero with local application communication enabled. Selected
        metadata is retained with its local database and item version.
        Attachments require a separate import.
      </p>
      <button className="ui-button" disabled={busy} onClick={() => load(null)}>
        Choose collection
      </button>
      <ErrorNotice error={error} />
      {preview && (
        <>
          <p className="text-xs">
            Database{" "}
            {preview.serverId ?? "legacy (no server identity supplied)"} · API
            v3 · {preview.collection || "collections"}
          </p>
          {preview.items
            .filter(
              (item) =>
                !["attachment", "note"].includes(item.data.itemType ?? ""),
            )
            .map((item) =>
              preview.collection ? (
                <label className="block" key={item.key}>
                  <input
                    type="checkbox"
                    checked={selected.includes(item.key)}
                    onChange={(event) =>
                      setSelected((keys) =>
                        event.target.checked
                          ? [...keys, item.key]
                          : keys.filter((key) => key !== item.key),
                      )
                    }
                  />{" "}
                  {item.data.title || item.key} · v{item.version}
                </label>
              ) : (
                <p key={item.key}>
                  <button
                    className="underline"
                    disabled={busy}
                    onClick={() => load(item.key)}
                  >
                    {item.data.name || item.key}
                  </button>
                </p>
              ),
            )}
          {preview.hasMore && (
            <button
              className="ui-button"
              disabled={busy}
              onClick={() =>
                load(preview.collection || null, preview.start + 100)
              }
            >
              Next 100 (clears selection)
            </button>
          )}
          {preview.collection && (
            <button
              className="ui-button"
              disabled={busy || !selected.length}
              onClick={() =>
                void run(async () => {
                  await projectIntegrations.importZotero(
                    workspaceId,
                    preview,
                    selected,
                  );
                  setNotice(
                    `Retained ${selected.length} selected metadata records in Sources.`,
                  );
                  setSelected([]);
                })
              }
            >
              Import selected metadata
            </button>
          )}
        </>
      )}
      {notice && <p role="status">{notice}</p>}
    </details>
  );
}

export default function Citations({
  workspaceId,
  paths,
  sourcePath,
  sourceText,
  checkpointId,
  disabled,
  onInsert,
}: {
  workspaceId: string;
  paths: string[];
  sourcePath: string;
  sourceText: string;
  checkpointId: string | null;
  disabled: boolean;
  onInsert: (text: string) => void;
}) {
  const [bibliographies, setBibliographies] = useState(paths.join("\n"));
  const [index, setIndex] = useState<CitationIndex | null>(null);
  const [basis, setBasis] = useState("");
  const [query, setQuery] = useState("");
  const { run, busy, error } = useAction();
  const signature = JSON.stringify([
    sourceText,
    sourcePath,
    checkpointId,
    bibliographies,
  ]);
  const rows =
    index?.entries.filter((entry) =>
      `${entry.key} ${entry.title} ${entry.author} ${entry.year}`
        .toLowerCase()
        .includes(query.toLowerCase()),
    ) ?? [];
  return (
    <details className="border-b px-3 py-2 text-sm">
      <summary>Citations</summary>
      <div className="space-y-3 max-h-80 overflow-auto py-2">
        <Field label="Bibliography files (one project path per line)">
          <textarea
            className="ui-input w-full"
            rows={2}
            value={bibliographies}
            onChange={(event) => setBibliographies(event.target.value)}
            placeholder="references.bib"
          />
        </Field>
        <button
          className="ui-button"
          disabled={busy || !bibliographies.trim()}
          onClick={() =>
            void run(async () => {
              const result = await projectIntegrations.citations(workspaceId, {
                bibliographies: bibliographies
                  .split("\n")
                  .map((s) => s.trim())
                  .filter(Boolean),
                sourcePath,
                sourceText,
                checkpointId,
              });
              setIndex(result);
              setBasis(signature);
            })
          }
        >
          Index and check citations
        </button>
        <ErrorNotice error={error} />
        {index && (
          <>
            <p className="text-xs">
              {basis === signature
                ? "Checked this editor text"
                : "Text or selection changed; check again"}
              . {index.coverage}
            </p>
            {index.missing.length > 0 ? (
              <p role="status">Missing keys: {index.missing.join(", ")}</p>
            ) : (
              <p>No missing keys in the checked text.</p>
            )}
            {index.warnings.map((warning) => (
              <p key={warning}>{warning}</p>
            ))}
            <input
              className="ui-input"
              aria-label="Search citations"
              placeholder="Author, title, or key"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
            {rows.slice(0, 100).map((entry, i) => (
              <article
                key={`${entry.path}/${entry.key}/${i}`}
                className="border-b py-2"
              >
                <p>
                  <strong>{entry.key}</strong> · {entry.title} · {entry.author}{" "}
                  {entry.year}
                </p>
                <button
                  className="ui-button"
                  disabled={disabled || !/^[\w.:/+\-]+$/.test(entry.key)}
                  onClick={() =>
                    onInsert(
                      /\.(md|qmd|rmd)$/i.test(sourcePath)
                        ? `[@${entry.key}]`
                        : `\\cite{${entry.key}}`,
                    )
                  }
                >
                  Insert citation
                </button>{" "}
                <button
                  className="ui-button"
                  onClick={() =>
                    appEvents.emit("open-file", {
                      workspaceId,
                      location: { path: entry.path, line: entry.line },
                    })
                  }
                >
                  Open bibliography
                </button>{" "}
                {entry.doi && (
                  <button
                    className="ui-button"
                    onClick={() =>
                      void open(
                        `https://doi.org/${encodeURIComponent(entry.doi!)}`,
                      )
                    }
                  >
                    Open DOI
                  </button>
                )}
                {entry.url?.startsWith("https://") && (
                  <button
                    className="ui-button"
                    onClick={() => void open(entry.url!)}
                  >
                    Open source
                  </button>
                )}
                {entry.warnings.map((warning) => (
                  <p className="text-xs" key={warning}>
                    {warning}
                  </p>
                ))}
              </article>
            ))}
            {rows.length > 100 && (
              <p>Showing 100 of {rows.length}; narrow the search.</p>
            )}
          </>
        )}
        <ZoteroImport workspaceId={workspaceId} />
      </div>
    </details>
  );
}
