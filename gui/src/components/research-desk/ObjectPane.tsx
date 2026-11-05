import { useEffect, useState } from "react";
import {
  deskClient,
  type DatasetVersion,
  type OpenResearchObject,
  type ResearchObject,
} from "../../lib/deskClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import ReportViewer from "../ReportViewer";
import { addContextObject } from "../WorkspaceContextTray";
import ConnectorNotice, { datasetConnector } from "./ConnectorNotice";
import { button, muted } from "./shared";
export default function ObjectPane({
  workspaceId,
  object,
  onClose,
  onPin,
  onError,
}: {
  workspaceId: string;
  object: OpenResearchObject;
  onClose: () => void;
  onPin?: () => void;
  onError: (s: string) => void;
}) {
  const [source, setSource] = useState<ResearchObject | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let alive = true;
    setSource(null);
    setFailed(false);
    void deskClient
      .read(workspaceId, object)
      .then((s) => {
        if (alive) setSource(s);
      })
      .catch((e) => {
        if (!alive) return;
        setFailed(true);
        onError(workbenchErrorMessage(e));
      });
    return () => {
      alive = false;
    };
  }, [workspaceId, object, onError, attempt]);
  let record: Record<string, unknown> | null = null;
  try {
    record = source
      ? (JSON.parse(source.text) as Record<string, unknown>)
      : null;
  } catch {
    /* Plain text is a supported object body. */
  }
  return (
    <section
      aria-label="Research item"
      className="flex min-h-0 min-w-0 flex-1 flex-col rounded border bg-white dark:bg-neutral-950"
    >
      <header className="flex flex-wrap items-center gap-2 border-b p-3">
        <h2 className="min-w-0 flex-1 truncate font-semibold">
          {source?.title ??
            (failed ? "This item could not be opened" : "Opening source…")}
        </h2>
        {failed && (
          <button
            className={button}
            onClick={() => setAttempt((count) => count + 1)}
          >
            Try again
          </button>
        )}
        <button
          className={button}
          onClick={() => addContextObject(workspaceId, object)}
        >
          Add to conversation
        </button>
        {onPin && (
          <button className={button} onClick={onPin}>
            Pin comparison
          </button>
        )}
        <button className={button} onClick={onClose}>
          Close
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        <p className={muted}>
          {object.kind} · {source?.provenance} · {source?.access} ·{" "}
          {source?.completeness}
        </p>
        <p className={`${muted} mb-4 break-all`}>Version {object.revision}</p>
        {object.kind === "dataset" && record && (
          <DatasetNotice record={record} />
        )}
        {record && typeof record === "object" ? (
          <dl className="space-y-4">
            {Object.entries(record).map(([key, value]) => (
              <div key={key}>
                <dt className="text-xs font-semibold">
                  {key.replace(/([A-Z])/g, " $1")}
                </dt>
                <dd className="mt-1 whitespace-pre-wrap break-words text-sm">
                  {typeof value === "string" ? (
                    <ReportViewer markdown={value} />
                  ) : (
                    <pre className="whitespace-pre-wrap text-xs">
                      {JSON.stringify(value, null, 2)}
                    </pre>
                  )}
                </dd>
              </div>
            ))}
          </dl>
        ) : (
          <ReportViewer markdown={source?.text ?? ""} />
        )}{" "}
        {source?.truncated && (
          <p className="mt-4 text-xs text-warning">
            Only part of this record is shown.
          </p>
        )}
      </div>
    </section>
  );
}
/** Statistical sources require their notice wherever their data are shown. */
function DatasetNotice({ record }: { record: Record<string, unknown> }) {
  const version = record as unknown as DatasetVersion;
  const connector = datasetConnector(version);
  if (!connector) return null;
  return (
    <div className="mb-4">
      <ConnectorNotice
        connector={connector}
        recorded={version.connector?.notice}
      />
    </div>
  );
}
