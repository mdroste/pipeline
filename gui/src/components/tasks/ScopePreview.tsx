import { Fragment, useEffect, useState } from "react";
import { taskClient } from "../../lib/taskClient";
import { connections } from "../../lib/connections";
import { reviewConnectionLabel } from "../../lib/connections";
import type { Json, SessionChoice, Step, TaskRun } from "../../lib/taskTypes";
import { childLists } from "./language";
import { localZone, scheduledTime, zoneLabel } from "./schedulePresentation";

export function flattenSteps(steps: Step[]): Step[] {
  return steps.flatMap((step) => [
    step,
    ...childLists(step).flatMap(flattenSteps),
  ]);
}
const object = (v: Json | undefined) =>
  v && typeof v === "object" && !Array.isArray(v) ? v : null;

export default function ScopePreview({
  run,
  background,
  onBackgroundSetup,
}: {
  run: TaskRun;
  background: boolean | null;
  onBackgroundSetup?: () => void;
}) {
  const [session, setSession] = useState<SessionChoice | null>(null);
  const [copyStatus, setCopyStatus] = useState("");
  useEffect(() => {
    let current = true;
    setSession(null);
    if (run.scope.sessionId && !run.scope.conversationTitle)
      void taskClient
        .sessions()
        .then((items) => {
          if (current)
            setSession(
              items.find((item) => item.id === run.scope.sessionId) ?? null,
            );
        })
        .catch(() => undefined);
    return () => {
      current = false;
    };
  }, [run.scope.sessionId, run.scope.conversationTitle]);
  const steps = flattenSteps(run.chain.steps);
  const files = steps.flatMap((step) =>
    step.kind === "snapshot" &&
    step.input.kind === "literal" &&
    typeof object(step.input.value)?.path === "string"
      ? [String(object(step.input.value)!.path)]
      : [],
  );
  const outputs = [
    ...new Set(
      steps.flatMap((step) =>
        step.kind === "snapshot"
          ? [step.filename]
          : step.kind === "review"
            ? ["Review report and findings"]
            : step.kind === "workspace"
              ? ["Assistant reply in the conversation"]
              : [],
      ),
    ),
  ];
  const trigger = object(run.inputs.__trigger);
  const timezone =
    trigger?.kind === "calendar" && typeof trigger.timezone === "string"
      ? trigger.timezone
      : localZone();
  return (
    <div className="task-scope">
      <h3>Ready to start</h3>
      <dl>
        <dt>Conversation</dt>
        <dd>
          {run.scope.conversationTitle ||
            session?.title ||
            (run.scope.sessionId
              ? "Saved conversation"
              : "No conversation steps")}
        </dd>
        <dt>Project</dt>
        <dd>
          {run.scope.projectName || session?.workspaceName || "No project"}
        </dd>
        <dt>Folder</dt>
        <dd>{run.scope.runtimeRoot || "Automation files and results"}</dd>
        {!!files.length && (
          <>
            <dt>Input files</dt>
            <dd>
              {files.map((file) => (
                <div key={file}>
                  <strong>{file.split(/[\\/]/).pop()}</strong>
                  <br />
                  {file}
                </div>
              ))}
            </dd>
          </>
        )}
        <dt>Review designs</dt>
        <dd>
          {Object.values(run.scope.profiles)
            .map((profile) => profile.profileName)
            .join(", ") || "None"}
        </dd>
        {!!run.scope.connectionId && (
          <>
            <dt>Assistant connection</dt>
            <dd>
              {connections.find((c) => c.id === run.scope.connectionId)
                ?.label || run.scope.connectionId}
            </dd>
          </>
        )}
        {steps
          .filter((step) => step.kind === "workspace" && step.model)
          .map((step) => (
            <Fragment key={step.id}>
              <dt>{step.label} model</dt>
              <dd>{step.kind === "workspace" && step.model}</dd>
            </Fragment>
          ))}
        {!!outputs.length && (
          <>
            <dt>Expected output</dt>
            <dd>{outputs.join(" · ")}</dd>
          </>
        )}
        <dt>Limits</dt>
        <dd>
          {run.chain.limits.maxActions} actions ·{" "}
          {run.chain.limits.actionTimeoutSecs / 60} minutes per action ·{" "}
          {run.chain.limits.deadlineHours} hour deadline
        </dd>
      </dl>
      {Object.entries(run.scope.reviewModels ?? {}).map(([id, entries]) => (
        <div key={id}>
          <h4>
            {run.scope.profiles[id]?.profileName || "Review"} · Models and
            connections
          </h4>
          <ul>
            {entries.map((entry, i) => (
              <li key={i}>
                {entry.step_labels.join(", ") || entry.label}:{" "}
                {reviewConnectionLabel(entry.provider, entry.transport)} ·{" "}
                {entry.model.mode === "pinned"
                  ? entry.model.model
                  : "Automatic at launch"}
                {entry.note ? ` · ${entry.note}` : ""}
              </li>
            ))}
          </ul>
        </div>
      ))}
      <p className="task-muted">
        Review steps use the designs and model settings saved with this
        automation. Connections use the account signed in when the step runs.
      </p>
      <section aria-label="Where and when this runs" className="task-readiness">
        <h4>Runs on this computer</h4>
        <p>Keep this computer awake and connected while the automation runs.</p>
        <p>
          Background mode:{" "}
          <strong>
            {background === null
              ? "Could not check"
              : background
                ? "On"
                : "Off"}
          </strong>
          .{" "}
          {background
            ? "You can close the window. Keep Pipeline running in the tray."
            : "Keep the Pipeline window open."}
        </p>
        {background !== true && onBackgroundSetup && (
          <button type="button" onClick={onBackgroundSetup}>
            Set up background running
          </button>
        )}
        <p>
          <strong>Next run:</strong>{" "}
          {trigger?.kind === "now"
            ? "When you start"
            : scheduledTime(run.dueAt, timezone)}{" "}
          · {zoneLabel(timezone, run.dueAt ?? undefined)}
        </p>
        <p className="task-muted">
          Missed runs are combined into one when Pipeline resumes. Each run
          finishes before the next starts. Each scheduled run starts a separate
          conversation.
        </p>
      </section>
      <details>
        <summary>Details</summary>
        <p>Conversation ID: {run.scope.sessionId || "None"}</p>
        <p>Project ID: {run.scope.workspaceId || "None"}</p>
        <button
          type="button"
          onClick={async () => {
            try {
              await navigator.clipboard.writeText(
                JSON.stringify(
                  {
                    conversationId: run.scope.sessionId,
                    projectId: run.scope.workspaceId,
                  },
                  null,
                  2,
                ),
              );
              setCopyStatus("IDs copied.");
            } catch {
              setCopyStatus(
                "Could not copy the IDs. Select them above to copy.",
              );
            }
          }}
        >
          Copy IDs
        </button>
        {copyStatus && <p role="status">{copyStatus}</p>}
      </details>
    </div>
  );
}
