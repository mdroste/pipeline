import { flushDrafts } from "../../lib/flushDrafts";
import { useEffect, useState } from "react";
import {
  documentType,
  documentTypeLabel,
  documentTypes,
  type DocumentType,
} from "../../lib/documentTypes";
import type {
  DocumentVersion,
  PaperWithRevision,
  ResearchPaper,
} from "../../lib/workbench/researchTypes";
import { researchClient } from "../../lib/workbench/researchClient";
import { projectClient } from "../../lib/projectClient";
import { button, input, formatDate, op, type SurfaceApi } from "./shared";

export default function DocumentControls({
  data,
  workspaceId,
  revisionId,
  onSelect: selectRevision,
  run,
  reportError,
  onImport,
}: Pick<SurfaceApi, "data" | "workspaceId" | "run" | "reportError"> & {
  revisionId: string | null;
  onSelect: (id: string | null) => void;
  onImport: () => void;
}) {
  const onSelect = (id: string | null) => {
    try {
      flushDrafts();
      selectRevision(id);
    } catch (error) {
      reportError(error);
    }
  };
  const [browse, setBrowse] = useState(false);
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState("");
  const [archive, setArchive] = useState(false);
  const [historicalOwner, setHistoricalOwner] = useState<{
    revisionId: string;
    paperId: string;
  } | null>(null);
  const direct = data.papers.find(
    (document) => document.revision?.id === revisionId,
  );
  const selected =
    direct ??
    data.papers.find(
      (document) =>
        historicalOwner?.revisionId === revisionId &&
        document.paper.id === historicalOwner.paperId,
    );
  const [history, setHistory] = useState<{
    paperId: string;
    versions: DocumentVersion[];
  } | null>(null);
  useEffect(() => {
    let live = true;
    if (revisionId && !direct) {
      void projectClient
        .read(workspaceId, revisionId)
        .then((view) => {
          if (live)
            setHistoricalOwner({ revisionId, paperId: view.revision.paperId });
        })
        .catch((error) => {
          if (live) reportError(error);
        });
    }
    return () => {
      live = false;
    };
  }, [workspaceId, revisionId, direct?.paper.id, reportError]);
  useEffect(() => {
    let live = true;
    if (selected)
      void researchClient
        .documentVersions(workspaceId, selected.paper.id)
        .then((versions) => {
          if (live) setHistory({ paperId: selected.paper.id, versions });
        })
        .catch((error) => {
          if (live) reportError(error);
        });
    return () => {
      live = false;
    };
  }, [workspaceId, selected?.paper.id, selected?.paper.updatedAt, reportError]);
  const ordered = [...data.papers]
    .filter((document) => document.revision)
    .sort(
      (a, b) =>
        Number(Boolean(b.paper.pinned)) - Number(Boolean(a.paper.pinned)) ||
        b.paper.updatedAt?.localeCompare(a.paper.updatedAt) ||
        a.paper.title.localeCompare(b.paper.title),
    );
  const visible = ordered.filter(
    ({ paper }) =>
      Boolean(paper.archived) === archive &&
      (!filter || documentType(paper) === filter) &&
      paper.title.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
  );
  const versions =
    history?.paperId === selected?.paper.id ? (history?.versions ?? []) : [];
  const update = (paper: ResearchPaper, patch: Partial<ResearchPaper>) =>
    run(() =>
      researchClient.updateDocument({
        workspaceId,
        paperId: paper.id,
        expectedUpdatedAt: paper.updatedAt,
        title: paper.title,
        documentType: documentType(paper),
        pinned: Boolean(paper.pinned),
        archived: Boolean(paper.archived),
        ...patch,
        operationId: op(),
      }),
    );
  return (
    <div className="mb-3 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <select
          aria-label="Document"
          className={`${input} max-w-sm`}
          value={selected?.revision?.id ?? revisionId ?? ""}
          onChange={(event) => onSelect(event.target.value || null)}
        >
          <option value="">Choose a document</option>
          {documentTypes.map((type) => {
            const documents = ordered.filter(
              (item) =>
                documentType(item.paper) === type.id &&
                (!item.paper.archived || item.paper.id === selected?.paper.id),
            );
            return documents.length ? (
              <optgroup key={type.id} label={type.label}>
                {documents.map((item) => (
                  <option key={item.paper.id} value={item.revision!.id}>
                    {item.paper.pinned ? "★ " : ""}
                    {item.paper.title}
                    {item.paper.archived ? " (archived)" : ""}
                  </option>
                ))}
              </optgroup>
            ) : null;
          })}
          {!selected && revisionId && (
            <option value={revisionId}>Earlier version</option>
          )}
        </select>
        {selected && (
          <select
            aria-label="Document version"
            className={`${input} max-w-xs`}
            value={revisionId ?? ""}
            onChange={(event) => onSelect(event.target.value)}
          >
            {!versions.some((version) => version.id === revisionId) &&
              revisionId && (
                <option value={revisionId}>
                  {revisionId === selected.paper.currentRevisionId
                    ? "Current version"
                    : "Selected version"}
                </option>
              )}
            {versions.map((version) => (
              <option key={version.id} value={version.id}>
                {formatDate(version.capturedAt)}
                {version.id === selected.paper.currentRevisionId
                  ? " · Current"
                  : ""}
              </option>
            ))}
          </select>
        )}
        <button
          className={button}
          onClick={() => setBrowse(!browse)}
          aria-expanded={browse}
        >
          Browse documents
        </button>
        <button className={button} onClick={onImport}>
          Add document
        </button>
      </div>
      {browse && (
        <section
          aria-label="Document browser"
          className="rounded-lg border border-line bg-surface p-3"
        >
          <div className="mb-3 flex flex-wrap items-center gap-2">
            <input
              aria-label="Search documents"
              className={`${input} max-w-xs`}
              placeholder="Find a document…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <select
              aria-label="Filter document type"
              className={`${input} max-w-40`}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            >
              <option value="">All types</option>
              {documentTypes.map((type) => (
                <option key={type.id} value={type.id}>
                  {type.label}
                </option>
              ))}
            </select>
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={archive}
                onChange={(e) => setArchive(e.target.checked)}
              />
              Archived
            </label>
          </div>
          <div className="max-h-60 overflow-y-auto">
            {visible.length ? (
              visible.map((document) => (
                <button
                  key={document.paper.id}
                  className={`flex w-full items-center justify-between gap-4 rounded px-3 py-2 text-left text-sm hover:bg-raised ${selected?.paper.id === document.paper.id ? "bg-raised font-medium" : ""}`}
                  onClick={() => {
                    onSelect(document.revision!.id);
                    setBrowse(false);
                  }}
                >
                  <span className="truncate">
                    {document.paper.pinned ? "★ " : ""}
                    {document.paper.title}
                  </span>
                  <span className="shrink-0 text-xs text-ink-muted">
                    {documentTypeLabel(document.paper)}
                  </span>
                </button>
              ))
            ) : (
              <p className="p-3 text-sm text-ink-muted">
                No matching documents.
              </p>
            )}
          </div>
        </section>
      )}
      {selected && (
        <DocumentDetails
          key={`${selected.paper.id}:${selected.paper.updatedAt}`}
          document={selected}
          update={update}
        />
      )}
      {selected &&
        revisionId &&
        selected.paper.currentRevisionId !== revisionId && (
          <div className="flex items-center gap-3 text-xs text-ink-muted">
            <span>Viewing an earlier version.</span>
            <button
              className={button}
              onClick={() =>
                void update(selected.paper, { currentRevisionId: revisionId })
              }
            >
              Make current version
            </button>
          </div>
        )}
    </div>
  );
}

function DocumentDetails({
  document,
  update,
}: {
  document: PaperWithRevision;
  update: (
    paper: ResearchPaper,
    patch: Partial<ResearchPaper>,
  ) => Promise<void>;
}) {
  const { paper } = document;
  const [title, setTitle] = useState(paper.title);
  const [type, setType] = useState<DocumentType>(documentType(paper));
  const [pinned, setPinned] = useState(Boolean(paper.pinned));
  return (
    <details className="text-xs text-ink-muted">
      <summary className="w-fit cursor-pointer">
        {documentTypeLabel(paper)} · Document details
      </summary>
      <form
        className="mt-2 flex flex-wrap items-end gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          void update(paper, {
            title: title.trim(),
            documentType: type,
            pinned,
          });
        }}
      >
        <label className="space-y-1">
          Name
          <input
            aria-label="Document name"
            className={input}
            maxLength={500}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="space-y-1">
          Document type
          <select
            aria-label="Document type"
            className={input}
            value={type}
            onChange={(e) => setType(e.target.value as DocumentType)}
          >
            {documentTypes.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </label>
        <label className="flex min-h-8 items-center gap-2">
          <input
            type="checkbox"
            checked={pinned}
            onChange={(e) => setPinned(e.target.checked)}
          />
          Pin
        </label>
        <button
          className={button}
          disabled={
            !title.trim() ||
            (title.trim() === paper.title &&
              type === documentType(paper) &&
              pinned === Boolean(paper.pinned))
          }
        >
          Save details
        </button>
        <button
          type="button"
          className={button}
          onClick={() => void update(paper, { archived: !paper.archived })}
        >
          {paper.archived ? "Restore document" : "Archive document"}
        </button>
      </form>
    </details>
  );
}
