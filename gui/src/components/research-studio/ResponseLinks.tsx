import { useEffect, useState } from "react";
import {
  projectClient,
  type Checkpoint,
  type ProjectRecord,
  type SnapshotPreview,
} from "../../lib/projectClient";
import { reference, type ContextItem } from "../../lib/deskClient";
import {
  studioClient,
  type ResponseRecord,
  type ResponseDecision,
} from "../../lib/studioClient";
import { researchDraftInstruction } from "../../lib/researchContext";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import SourceDiff from "../file-workspace/SourceDiff";
import {
  button,
  panel,
  useAction,
  ErrorNotice,
  type StudioProps,
} from "./shared";

function ChangeEvidence({
  workspaceId,
  change,
}: {
  workspaceId: string;
  change: ProjectRecord<Checkpoint>;
}) {
  const paths = [
    ...new Set([
      ...Object.keys(change.body.base),
      ...Object.keys(change.body.proposed),
    ]),
  ].filter(
    (path) => change.body.base[path]?.hash !== change.body.proposed[path]?.hash,
  );
  const [path, setPath] = useState(paths[0] ?? "");
  const [sides, setSides] = useState<(SnapshotPreview | null)[] | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    setSides(null);
    setError("");
    void Promise.all(
      [change.body.base[path]?.hash, change.body.proposed[path]?.hash].map(
        (hash) =>
          hash
            ? projectClient.preview(workspaceId, hash)
            : Promise.resolve(null),
      ),
    )
      .then((value) => {
        if (live) setSides(value);
      })
      .catch((cause) => {
        if (live) setError(workbenchErrorMessage(cause));
      });
    return () => {
      live = false;
    };
  }, [workspaceId, change.id, change.revision, path]);
  return (
    <details>
      <summary>Inspect proposed change · {paths.length} files</summary>
      <select
        aria-label="Change evidence file"
        value={path}
        onChange={(event) => setPath(event.target.value)}
      >
        {paths.map((path) => (
          <option key={path}>{path}</option>
        ))}
      </select>
      {error && <p role="alert">{error}</p>}
      {sides &&
        (sides.every((side) => !side || side.text != null) ? (
          <SourceDiff
            path={path}
            before={sides[0]?.text ?? ""}
            after={sides[1]?.text ?? ""}
          />
        ) : (
          <p>Binary change; inspect the artifacts in Review edits.</p>
        ))}
    </details>
  );
}

export default function ResponseLinks({
  workspaceId,
  data,
  record,
  decision,
  onSaved,
  onRefresh,
  onAsk,
  onRelatedTarget,
}: Pick<
  StudioProps,
  "workspaceId" | "data" | "onRefresh" | "onAsk" | "onRelatedTarget"
> & {
  record: ProjectRecord<ResponseRecord>;
  decision: ResponseDecision;
  onSaved: (record: ProjectRecord<ResponseRecord>) => void;
}) {
  const { busy, error, run } = useAction();
  const source = record.body.source;
  const clean =
    JSON.stringify(decision) === JSON.stringify(record.body.decision);
  const changes = data.changes.filter(
    (change) => change.body.taskId === decision.taskId,
  );
  const applications = data.applications.filter((application) =>
    changes.some((change) => change.id === application.body.checkpointId),
  );
  const linked = data.applications.find(
    (application) => application.id === decision.applicationId,
  );
  const check = data.executions.find(
    (execution) => execution.id === decision.executionId,
  );
  const save = async (next: ResponseDecision) => {
    const saved = await studioClient.mutate(workspaceId, {
      action: "saveResponse",
      id: record.id,
      expectedRevision: record.revision,
      response: next,
    });
    onSaved(saved);
    await onRefresh();
  };
  const draftReply = async () => {
    const snapshot = await studioClient.mutate(workspaceId, {
      action: "responseContext",
      id: record.id,
      expectedRevision: record.revision,
    });
    const context: ContextItem[] = [
      { role: "supporting", object: reference(snapshot) },
    ];
    if (decision.manuscriptRevisionId) {
      const { revision } = await projectClient.read(
        workspaceId,
        decision.manuscriptRevisionId,
      );
      context.push({
        role: "main",
        object: {
          kind: "paper",
          id: revision.id,
          revision: revision.contentHash,
        },
      });
    }
    await onAsk?.(
      `Draft a reply to response ${decision.number} using its selected comment, decision, accepted change, and check receipts. Identify remaining work and unsupported claims. ${researchDraftInstruction}`,
      context,
    );
  };
  return (
    <section className={panel} aria-label="Response evidence and work">
      <h3 className="font-semibold">
        {source.finding
          ? "Assistant finding"
          : "External referee or editor comment"}
      </h3>
      <p className="whitespace-pre-wrap text-sm">
        {source.comment?.text ?? source.finding?.title}
      </p>
      <ErrorNotice error={error} />
      <div className="flex flex-wrap gap-2">
        {decision.taskId ? (
          <button
            className={button}
            onClick={() => onRelatedTarget?.("tasks", decision.taskId!)}
          >
            Open revision task
          </button>
        ) : (
          <button
            className={button}
            disabled={busy || !clean}
            onClick={() =>
              void run(async () => {
                const task = await projectClient.mutate(workspaceId, {
                  action: "createTask",
                  objective: `Respond to comment ${decision.number}: ${decision.intendedResponse || source.comment?.text || source.finding?.title || "Investigate the finding"}\nSource response: ${record.id}, revision ${record.revision}`,
                  anchorId: decision.evidenceAnchorIds[0] ?? null,
                  expectedOutputs: [
                    "Proposed manuscript change",
                    "Response draft",
                  ],
                  expectedChecks: decision.resolvingCheck
                    ? [decision.resolvingCheck]
                    : [],
                });
                await save({ ...decision, taskId: task.id });
              })
            }
          >
            Create linked revision task
          </button>
        )}
        {onAsk && (
          <button
            className={button}
            disabled={busy || !clean}
            onClick={() => void run(draftReply)}
          >
            {linked?.body.state === "applied"
              ? "Draft reply from accepted change"
              : "Draft reply with selected evidence"}
          </button>
        )}
      </div>
      {!clean && (
        <p className="text-xs">
          Save the response decision before creating linked work or drafting a
          reply.
        </p>
      )}
      {changes.map((change) => (
        <div key={change.id} className="my-2">
          <button
            className={button}
            onClick={() => onRelatedTarget?.("edits", change.id)}
          >
            Review edits · {change.body.state}
          </button>
          <ChangeEvidence workspaceId={workspaceId} change={change} />
        </div>
      ))}
      {applications
        .filter(
          (application) =>
            application.body.state === "applied" &&
            application.id !== decision.applicationId,
        )
        .map((application) => (
          <button
            key={application.id}
            className={button}
            disabled={busy || !clean}
            onClick={() =>
              void run(() =>
                save({ ...decision, applicationId: application.id }),
              )
            }
          >
            Link accepted edit · {application.body.files.length} files
          </button>
        ))}
      {linked && (
        <p className="text-xs">
          Linked edit: {linked.body.state} ·{" "}
          {linked.body.files.map((file) => file.path).join(", ")}.{" "}
          {linked.body.checksValid
            ? "Recorded checks match the accepted combination."
            : "Checks do not establish the accepted combination; inspect or rerun."}
        </p>
      )}
      {check && (
        <details>
          <summary>Recorded check · {check.outcome}</summary>
          <p className="text-xs">
            {check.command.join(" ")} · {check.snapshotConsistency} input
            capture. Outcome applies to declared inputs; it does not resolve the
            objection.
          </p>
          <pre className="overflow-auto text-xs">
            {JSON.stringify(
              {
                id: check.id,
                inputs: check.inputManifest,
                outputs: check.outputManifest,
                validation: check.validation,
              },
              null,
              2,
            )}
          </pre>
        </details>
      )}
    </section>
  );
}
