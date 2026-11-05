// The Builds side panel: past builds, the chosen build's errors, and the tools
// that connect a build's PDF to its source. A chosen build is read-only history.
import { lazy, Suspense, useState } from "react";
import { Fold } from "../../ui";
import {
  studioClient,
  type BuildReceipt,
  type BuildRecord,
  type SyncResult,
} from "../../lib/studioClient";
import type { ProjectRecord } from "../../lib/projectClient";
import type { ResearchExecution } from "../../lib/workbenchTypes";
import { button, input, Field, Select, Inspect } from "./shared";

const PdfComparison = lazy(() => import("../file-workspace/PdfComparison"));

export default function ManuscriptBuilds({
  workspaceId,
  builds,
  receipts,
  shownBuild,
  receipt,
  compareBuild,
  comparison,
  busy,
  run,
  fileOpen,
  sourcePath,
  sourceLine,
  setSourceLine,
  pdfPage,
  setPdfPage,
  sync,
  syncCandidates,
  onSync,
  onPdfPoint,
  onSelect,
  onCompare,
  onInspected,
  onRefreshBuilds,
  onDocument,
  onJumpLine,
  prefix,
  onNavigate,
  onCandidate,
}: {
  workspaceId: string;
  builds: ResearchExecution[];
  receipts: ProjectRecord<BuildRecord | BuildReceipt>[];
  shownBuild: string;
  receipt?: BuildReceipt;
  compareBuild: string;
  comparison?: BuildReceipt;
  busy: boolean;
  run: (work: () => Promise<unknown>) => Promise<void>;
  fileOpen: boolean;
  sourcePath: string;
  sourceLine: string;
  setSourceLine: (line: string) => void;
  pdfPage: number;
  setPdfPage: (page: number) => void;
  sync: string;
  syncCandidates: SyncResult["candidates"];
  onSync: (request: {
    sourcePath?: string;
    line?: number;
    page?: number;
    x?: number;
    y?: number;
  }) => Promise<void>;
  onPdfPoint: (point: { page: number; x: number; y: number }) => Promise<void>;
  onSelect: (id: string) => void;
  onCompare: (id: string) => void;
  onInspected: (executionId: string) => Promise<void>;
  onRefreshBuilds: () => Promise<void>;
  onDocument: (revision: string) => void;
  onJumpLine: (line: number) => void;
  /** The build's folder, which turns its file names into project paths. */
  prefix: string;
  onNavigate: (path: string, line: number) => void;
  onCandidate: (path: string, line: number) => void;
}) {
  const [pdfX, setPdfX] = useState(0);
  const [pdfY, setPdfY] = useState(0);
  const lineNumber = Number(sourceLine);
  const validLine = Number.isInteger(lineNumber) && lineNumber > 0;
  const finished = receipts.map((r) => r.id);
  const known = new Set(builds.map((e) => `build_${e.id}`));
  return (
    <div className="manuscript-builds">
      {!builds.length && !receipts.length && (
        <p className="text-xs text-ink-muted">
          No builds yet. Choose Build to make the first one.
        </p>
      )}
      <ul aria-label="Builds" className="manuscript-build-list">
        {builds.slice(0, 12).map((e) => {
          const id = `build_${e.id}`;
          return (
            <li key={e.id}>
              <button
                aria-current={id === shownBuild ? "true" : undefined}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    if (finished.includes(id)) {
                      onSelect(id);
                      return;
                    }
                    await studioClient.mutate(workspaceId, {
                      action: "inspectBuild",
                      executionId: e.id,
                    });
                    await onInspected(e.id);
                  })
                }
              >
                <span>{e.outcome}</span>
                <span>{e.id.slice(-8)}</span>
              </button>
            </li>
          );
        })}
        {receipts
          .filter((r) => !known.has(r.id) && r.body.recordType === "receipt")
          .map((r) => {
            const body = r.body as BuildReceipt;
            return (
              <li key={r.id}>
                <button
                  aria-current={r.id === shownBuild ? "true" : undefined}
                  onClick={() => onSelect(r.id)}
                >
                  <span>{body.outcome}</span>
                  <span>{body.executionId.slice(-8)}</span>
                </button>
              </li>
            );
          })}
      </ul>
      {receipt && (
        <>
          {receipt.diagnostics.length > 0 && (
            <ul aria-label="Build messages" className="manuscript-messages">
              {receipt.diagnostics.map((d, i) => (
                <li key={i} data-severity={d.severity}>
                  <strong>{d.severity}</strong>
                  {d.path && d.line ? (
                    <button onClick={() => onNavigate(d.path!, d.line!)}>
                      {prefix}
                      {d.path}:{d.line}
                    </button>
                  ) : null}{" "}
                  {d.message}
                </li>
              ))}
            </ul>
          )}
          {receipt.pdf?.revision ? (
            <>
              <button
                className={button}
                onClick={() => onDocument(receipt.pdf!.revision!.id)}
              >
                Open this build’s PDF in Documents
              </button>
              <Fold title="Find between source and PDF">
                <div className="flex flex-wrap items-end gap-2">
                  <Field label="Source line">
                    <input
                      className={input}
                      type="number"
                      min={1}
                      value={sourceLine}
                      onChange={(e) => setSourceLine(e.target.value)}
                    />
                  </Field>
                  <button
                    className={button}
                    disabled={!fileOpen || !validLine}
                    onClick={() => onJumpLine(lineNumber)}
                  >
                    Go to line
                  </button>
                  <button
                    className={button}
                    disabled={busy || !receipt.mappingArtifactId || !validLine}
                    onClick={() =>
                      void run(() => onSync({ sourcePath, line: lineNumber }))
                    }
                  >
                    Find line in PDF
                  </button>
                </div>
                <div className="flex flex-wrap items-end gap-2">
                  {(
                    [
                      ["Page", pdfPage, setPdfPage],
                      ["X (PDF points)", pdfX, setPdfX],
                      ["Y (PDF points)", pdfY, setPdfY],
                    ] as const
                  ).map(([label, value, set]) => (
                    <Field key={label} label={label}>
                      <input
                        className={input}
                        type="number"
                        value={value}
                        onChange={(e) => set(Number(e.target.value))}
                      />
                    </Field>
                  ))}
                  <button
                    className={button}
                    disabled={busy || !receipt.mappingArtifactId}
                    onClick={() =>
                      void run(() =>
                        onSync({ page: pdfPage, x: pdfX, y: pdfY }),
                      )
                    }
                  >
                    Find source
                  </button>
                </div>
                {syncCandidates.map((c, i) => (
                  <div className="flex gap-2 text-xs" key={i}>
                    {c.page && (
                      <button
                        className={button}
                        onClick={() => setPdfPage(c.page!)}
                      >
                        Open build page {c.page}
                      </button>
                    )}
                    {c.sourcePath && c.line && (
                      <button
                        className={button}
                        onClick={() => onCandidate(c.sourcePath!, c.line!)}
                      >
                        Source {prefix}
                        {c.sourcePath}:{c.line}
                      </button>
                    )}
                  </div>
                ))}
                {sync && (
                  <pre className="max-h-40 overflow-auto whitespace-pre-wrap text-xs">
                    {sync}
                  </pre>
                )}
              </Fold>
              <Fold title="Compare with another build">
                <Select
                  label="Compare PDF with"
                  value={compareBuild}
                  onChange={onCompare}
                  options={receipts
                    .filter(
                      (r) =>
                        r.id !== shownBuild &&
                        (r.body as BuildReceipt).pdf?.revision,
                    )
                    .map((r) => ({
                      value: r.id,
                      label: (r.body as BuildReceipt).executionId,
                    }))}
                />
                {comparison?.pdf?.revision && (
                  <div className="h-[36rem]">
                    <Suspense fallback={<p>Loading PDF comparison…</p>}>
                      <PdfComparison
                        workspaceId={workspaceId}
                        primary={{
                          id: receipt.pdf.revision.id,
                          title: "Selected build",
                        }}
                        secondary={{
                          id: comparison.pdf.revision.id,
                          title: "Comparison build",
                        }}
                        page={pdfPage}
                        onSource={(point) => void onPdfPoint(point)}
                      />
                    </Suspense>
                  </div>
                )}
              </Fold>
              <Fold title="Record a page check">
                <p className="text-xs text-ink-muted">
                  Compilation does not check the pages. Record the pages you
                  have read in this build.
                </p>
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await studioClient.mutate(workspaceId, {
                        action: "inspectPages",
                        executionId: receipt.executionId,
                        pages: [pdfPage],
                        note: "I checked this page in the compiled PDF.",
                      });
                      await onRefreshBuilds();
                    })
                  }
                >
                  Record inspection of build page {pdfPage}
                </button>
                {receipt.inspectedPages?.length ? (
                  <p className="text-xs">
                    Pages marked as inspected:{" "}
                    {receipt.inspectedPages.join(", ")}
                  </p>
                ) : null}
              </Fold>
            </>
          ) : (
            <p className="text-sm">
              This build did not produce a PDF. Select an earlier successful
              build to view its PDF.
            </p>
          )}
          <Inspect
            value={receipt.sourceManifest}
            label="Captured build dependencies"
          />
        </>
      )}
    </div>
  );
}
