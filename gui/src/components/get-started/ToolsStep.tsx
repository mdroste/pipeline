import { useEffect, useRef, useState } from "react";
import type { GetStartedPersona } from "../../lib/getStarted";
import type { DepsReport } from "../../lib/types";
import { executionClient } from "../../lib/workbench/executionClient";
import type {
  DetectedToolchain,
  ToolchainReport,
} from "../../lib/workbench/toolchains";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { Badge, Button } from "../../ui";
import {
  groupTools,
  pdfRows,
  REPLICATION_NOTE,
  TOOL_NOTES,
  type Tone,
} from "./guide";

function Row({
  tone,
  status,
  name,
  detail,
  path,
}: {
  tone: Tone;
  status: string;
  name: string;
  detail: string;
  path?: string;
}) {
  return (
    <li className="flex items-start gap-3 rounded-ui-md border border-line px-3 py-2.5">
      <span className="mt-2">
        <Badge kind={tone} label={status} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-ui-label">
          <span className="font-medium">{name}</span>
          <span className="text-ink-muted"> · {status}</span>
        </p>
        <p className="text-ui-meta text-ink-muted">{detail}</p>
        {path && (
          <p className="mt-0.5 break-all font-mono text-[11px] text-ink-muted">
            {path}
          </p>
        )}
      </div>
    </li>
  );
}

function ToolRow({ tool }: { tool: DetectedToolchain }) {
  const note = TOOL_NOTES[tool.kind];
  return (
    <Row
      tone="ok"
      name={tool.label || note.name}
      status={tool.version ? `Found · ${tool.version}` : "Found"}
      detail={[note.use, tool.note].filter(Boolean).join(" ")}
      path={tool.path}
    />
  );
}

/** Runs read-only toolchain detection only when asked. */
export default function ToolsStep({
  persona,
  dependencies,
  onOpenPdfSettings,
  headingId,
}: {
  persona: GetStartedPersona | null;
  dependencies: DepsReport | null;
  onOpenPdfSettings: () => void;
  headingId: string;
}) {
  const [report, setReport] = useState<ToolchainReport | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const detect = () => {
    setDetecting(true);
    setError(null);
    executionClient
      .detectToolchains(null)
      .then((next) => {
        if (!mounted.current) return;
        if (next) setReport(next);
        else setError("No result was returned.");
      })
      .catch((caught: unknown) => {
        if (mounted.current) setError(workbenchErrorMessage(caught));
      })
      .finally(() => {
        if (mounted.current) setDetecting(false);
      });
  };

  const groups = report ? groupTools(report, persona) : null;
  const pdf = pdfRows(dependencies);

  return (
    <section aria-labelledby={headingId} className="space-y-4">
      <div>
        <h3
          id={headingId}
          tabIndex={-1}
          className="text-ui-body font-semibold outline-none"
        >
          Tools on this computer
        </h3>
        <p className="mt-1 text-ui-label text-ink-muted">
          In a project, Pipeline can run your own scripts as local checks and
          record the software versions they used. {REPLICATION_NOTE}
        </p>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button variant="primary" disabled={detecting} onClick={detect}>
          {detecting
            ? "Checking this computer…"
            : report
              ? "Check again"
              : "Check this computer"}
        </Button>
        {!report && (
          <span className="text-ui-meta text-ink-muted">
            Looks in standard install folders and on your PATH. Installs nothing
            and does not start Stata or MATLAB.
          </span>
        )}
      </div>
      {error && (
        <p role="alert" className="text-ui-label text-danger">
          The check did not finish: {error}
        </p>
      )}
      {groups && (
        <div className="space-y-3" aria-live="polite">
          {(groups.featured.length > 0 || groups.missing.length > 0) && (
            <ul aria-label="Tools you work with" className="space-y-2">
              {groups.featured.map((tool) => (
                <ToolRow key={`${tool.kind}:${tool.path}`} tool={tool} />
              ))}
              {groups.missing.map((kind) => (
                <Row
                  key={kind}
                  tone="idle"
                  name={TOOL_NOTES[kind].name}
                  status="Not found"
                  detail={`Not in the standard folders or on your PATH. You can still enter its path when you set up a local check. ${TOOL_NOTES[kind].use}`}
                />
              ))}
            </ul>
          )}
          {groups.others.length > 0 && (
            <div>
              <p className="mb-1.5 text-ui-meta font-medium text-ink-muted">
                {groups.featured.length || groups.missing.length
                  ? "Also found on this computer"
                  : "Found on this computer"}
              </p>
              <ul aria-label="Other tools found" className="space-y-2">
                {groups.others.map((tool) => (
                  <ToolRow key={`${tool.kind}:${tool.path}`} tool={tool} />
                ))}
              </ul>
            </div>
          )}
          {report && report.tools.length === 0 && (
            <p className="text-ui-label text-ink-muted">
              No research tools were found. You can still enter a program's path
              when you set up a local check.
            </p>
          )}
          <p className="text-ui-meta text-ink-muted">{report?.boundary}</p>
        </div>
      )}
      <div>
        <h4 className="text-ui-label font-semibold">Reading PDFs</h4>
        {pdf.length ? (
          <ul aria-label="PDF tools" className="mt-2 space-y-2">
            {pdf.map((row) => (
              <Row key={row.name} {...row} />
            ))}
          </ul>
        ) : (
          <p className="mt-1 text-ui-meta text-ink-muted">
            PDF tool status appears after Pipeline checks its connections.
          </p>
        )}
        <Button variant="link" className="mt-2" onClick={onOpenPdfSettings}>
          Open PDF extraction settings
        </Button>
      </div>
    </section>
  );
}
