import { useState } from "react";
import { projectClient } from "../../lib/projectClient";
import { executionClient } from "../../lib/workbench/executionClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { operation } from "./useResearchPanelController";
import { compactHash, plainLabel } from "./format";
import RunOutput from "./RunOutput";
import ToolchainPicker from "./ToolchainPicker";
import {
  ADAPTER_HELP,
  ADAPTER_OPTIONS,
  adapterTemplate,
  guessPlatform,
  isUntouchedTemplate,
  STATA_CLEANUP_NOTICE,
  stataUsesLauncher,
} from "../../lib/workbench/toolchains";
import type { useResearchPanelController } from "./useResearchPanelController";
import type {
  ConversationSnapshot,
  ExecutionProfile,
} from "../../lib/workbenchTypes";

type Panel = ReturnType<typeof useResearchPanelController>;
type Props = Pick<
  Panel,
  | "busy"
  | "act"
  | "workspace"
  | "profileName"
  | "setProfileName"
  | "adapter"
  | "setAdapter"
  | "argv"
  | "setArgv"
  | "inputs"
  | "setInputs"
  | "outputs"
  | "setOutputs"
  | "profiles"
  | "hostPreview"
  | "setHostPreview"
  | "executions"
> & {
  workspaceId: string;
  snapshot: ConversationSnapshot | null;
  resultsView: "all" | "runs" | "profiles";
  onError: (message: string) => void;
};

/** The backend's limit on one run, in seconds. */
const TIMEOUT_MAX = 7_200;
const TIMEOUT_DEFAULT = 300;

const adapterLabel = (adapter: string) =>
  ADAPTER_OPTIONS.find((option) => option.value === adapter)?.label ?? adapter;

function lines(value: string) {
  return value
    .split("\n")
    .map((item) => item.trim())
    .filter(Boolean);
}

export default function ResultsTab({
  busy,
  act,
  workspace,
  profileName,
  setProfileName,
  adapter,
  setAdapter,
  argv,
  setArgv,
  inputs,
  setInputs,
  outputs,
  setOutputs,
  profiles,
  hostPreview,
  setHostPreview,
  executions,
  workspaceId,
  snapshot,
  resultsView,
  onError,
}: Props) {
  const [timeout, setTimeoutText] = useState(String(TIMEOUT_DEFAULT));
  const [editing, setEditing] = useState<ExecutionProfile | null>(null);
  const editProfile = (profile: ExecutionProfile) => {
    setEditing(profile);
    setProfileName(profile.name);
    setAdapter(profile.adapter);
    setArgv(JSON.stringify(profile.argv));
    setInputs(profile.inputs.join("\n"));
    setOutputs(profile.outputs.join("\n"));
    setTimeoutText(String(profile.timeoutSeconds));
  };
  const reviewedProfile = hostPreview
    ? profiles.find((p) => p.id === hostPreview.preview.profileId)
    : undefined;
  const [openRuns, setOpenRuns] = useState<ReadonlySet<string>>(new Set());
  const toggleRun = (id: string, open: boolean) =>
    setOpenRuns((old) => {
      const next = new Set(old);
      if (open) next.add(id);
      else next.delete(id);
      return next;
    });
  return (
    <div className="space-y-4">
      <section hidden={resultsView === "runs"} className="space-y-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          {editing ? `Edit ${editing.name}` : "New execution profile"}
        </h3>
        <input
          aria-label="Profile name"
          value={profileName}
          onChange={(event) => setProfileName(event.target.value)}
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <select
          value={adapter}
          aria-label="Execution adapter"
          onChange={(event) => {
            const next = event.target.value as ExecutionProfile["adapter"];
            // Replace only unedited templates; typed arguments stay.
            const template = adapterTemplate(next, guessPlatform());
            if (isUntouchedTemplate(argv, adapter, "argv"))
              setArgv(JSON.stringify(template.argv));
            if (
              !outputs.trim() ||
              isUntouchedTemplate(outputs, adapter, "outputs")
            )
              setOutputs(template.outputs.join("\n"));
            setAdapter(next);
          }}
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        >
          {ADAPTER_OPTIONS.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
        <p className="text-[11px] text-gray-500">{ADAPTER_HELP[adapter]}</p>
        <textarea
          value={argv}
          onChange={(event) => setArgv(event.target.value)}
          rows={3}
          aria-label="Argument vector JSON"
          className="w-full rounded border bg-white p-2 font-mono text-[11px] dark:bg-neutral-950"
        />
        <ToolchainPicker
          workspaceId={workspaceId}
          adapter={adapter}
          argv={argv}
          setArgv={setArgv}
          disabled={busy}
          onError={onError}
        />
        <textarea
          aria-label="Input paths"
          value={inputs}
          onChange={(event) => setInputs(event.target.value)}
          rows={2}
          placeholder="Input paths, one relative path per line"
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <textarea
          aria-label="Expected output paths"
          value={outputs}
          onChange={(event) => setOutputs(event.target.value)}
          rows={2}
          placeholder="Expected output paths, one per line"
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <label className="block text-xs">
          Time limit (seconds)
          <input
            type="number"
            min={1}
            max={TIMEOUT_MAX}
            step={1}
            value={timeout}
            onChange={(event) => setTimeoutText(event.target.value)}
            className="mt-1 block w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
          />
        </label>
        <p className="text-[11px] text-gray-500">
          Pipeline stops the command when this runs out. The default is{" "}
          {TIMEOUT_DEFAULT} seconds; the longest allowed is{" "}
          {TIMEOUT_MAX.toLocaleString()} seconds, or two hours.
          {editing && " Saving a change clears the last test result."}
        </p>
        <button
          type="button"
          disabled={busy || !workspace?.root}
          onClick={() =>
            void act(async () => {
              const seconds = Number(timeout);
              if (
                !Number.isInteger(seconds) ||
                seconds < 1 ||
                seconds > TIMEOUT_MAX
              )
                throw new Error(
                  `Enter a time limit from 1 to ${TIMEOUT_MAX.toLocaleString()} seconds.`,
                );
              const parsed = JSON.parse(argv);
              if (
                !Array.isArray(parsed) ||
                !parsed.every((value) => typeof value === "string")
              )
                throw new Error(
                  "Enter command arguments as a JSON array of text values.",
                );
              await executionClient.saveExecutionProfile({
                profileId: editing?.id ?? null,
                workspaceId,
                name: profileName,
                adapter,
                argv: parsed,
                cwd: editing?.cwd ?? workspace!.root!,
                environment: editing?.environment ?? {},
                inputs: lines(inputs),
                timeoutSeconds: seconds,
                outputs: lines(outputs),
                expectedRevision: editing?.revision ?? null,
                operationId: operation("profile"),
              });
              setEditing(null);
            })
          }
          className="rounded border px-3 py-2 text-xs"
        >
          {editing ? "Save changes" : "Save profile"}
        </button>
        {editing && (
          <button
            type="button"
            className="ml-2 rounded border px-3 py-2 text-xs"
            onClick={() => setEditing(null)}
          >
            Stop editing
          </button>
        )}
        {!workspace?.root && (
          <p className="text-xs text-warning">
            Attach a project folder before setting up commands.
          </p>
        )}
      </section>
      <section hidden={resultsView === "runs"}>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Profiles
        </h3>
        <div className="mt-2 space-y-2">
          {profiles.map((profile) => (
            <div
              key={profile.id}
              className="rounded border bg-white p-3 dark:bg-neutral-950"
            >
              <div className="font-medium">{profile.name}</div>
              <pre className="mt-1 overflow-auto whitespace-pre-wrap text-[10px] text-gray-500">
                {JSON.stringify(profile.argv)}
              </pre>
              <p className="mt-1 text-[11px] text-gray-500">
                Time limit: {profile.timeoutSeconds} seconds
              </p>
              <div className="mt-2 flex items-center gap-2">
                <span
                  className={`text-[11px] ${profile.testStatus === "passed" ? "text-emerald-600" : "text-gray-500"}`}
                >
                  {plainLabel(profile.testStatus ?? "untested")}
                </span>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() =>
                    void act(async () => {
                      setHostPreview({
                        preview: await projectClient.previewExecution(
                          profile.id,
                        ),
                        testOnly: true,
                      });
                    })
                  }
                  className="rounded border px-2 py-1 text-[11px]"
                >
                  Test
                </button>
                <button
                  type="button"
                  disabled={busy || profile.testStatus !== "passed"}
                  onClick={() =>
                    void act(async () => {
                      setHostPreview({
                        preview: await projectClient.previewExecution(
                          profile.id,
                        ),
                        testOnly: false,
                      });
                    })
                  }
                  className="rounded border px-2 py-1 text-[11px]"
                >
                  Run
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => editProfile(profile)}
                  className="rounded border px-2 py-1 text-[11px]"
                >
                  Edit
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>
      {hostPreview && (
        <section className="space-y-2 rounded border border-amber-300 p-3">
          <h3 className="text-xs font-semibold">
            Review command before running
          </h3>
          <p className="text-xs">{hostPreview.preview.boundary}</p>
          {reviewedProfile && (
            <p className="text-xs">
              Pipeline stops this command after {reviewedProfile.timeoutSeconds}{" "}
              seconds.
            </p>
          )}
          <pre className="overflow-auto whitespace-pre-wrap text-[11px]">
            {JSON.stringify(
              {
                command: hostPreview.preview.command,
                cwd: hostPreview.preview.cwd,
                inputs: hostPreview.preview.inputs,
                launch: hostPreview.preview.launch,
                outputs: hostPreview.preview.outputs,
              },
              null,
              2,
            )}
          </pre>
          <button
            className="rounded border px-3 py-2 text-xs"
            disabled={busy}
            onClick={() =>
              void act(async () => {
                const pending = hostPreview;
                await projectClient.authorizeExecution(
                  pending.preview.profileId,
                  pending.preview.fingerprint,
                );
                setHostPreview(null);
                await executionClient.runExecution({
                  profileId: pending.preview.profileId,
                  sessionId: snapshot?.session.id ?? null,
                  testOnly: pending.testOnly,
                  operationId: operation("authorized-host-run"),
                });
              })
            }
          >
            Authorize this command and {hostPreview.testOnly ? "test" : "run"}
          </button>
          <button
            className="ml-2 rounded border px-3 py-2 text-xs"
            onClick={() => setHostPreview(null)}
          >
            Cancel
          </button>
        </section>
      )}
      <section hidden={resultsView === "profiles"}>
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          Run history
        </h3>
        <div className="mt-2 space-y-2">
          {executions.map((run) => (
            <details
              key={run.id}
              className="rounded border bg-white p-3 dark:bg-neutral-950"
              onToggle={(event) => toggleRun(run.id, event.currentTarget.open)}
            >
              <summary className="cursor-pointer text-xs">
                <span
                  className={
                    run.outcome === "completed"
                      ? "text-emerald-600"
                      : run.outcome === "running"
                        ? "text-blue-600"
                        : run.outcome === "queued"
                          ? "text-gray-500"
                          : "text-danger"
                  }
                >
                  {plainLabel(run.outcome)}
                </span>{" "}
                · {adapterLabel(run.adapter)} ·{" "}
                {compactHash(run.dependencyHash)}
              </summary>
              <div className="mt-2 space-y-2 text-[11px]">
                <pre className="overflow-auto whitespace-pre-wrap">
                  {JSON.stringify(run.command)}
                </pre>
                <p>
                  {run.inputManifest.snapshotConsistency ===
                  "captured_inputs_verified"
                    ? "Saved inputs checked · listed dependencies · runs with your computer’s permissions"
                    : `Snapshot: ${run.snapshotConsistency}`}{" "}
                  · exit {run.exitStatus ?? "—"}
                </p>
                {["queued", "running"].includes(run.outcome) && (
                  <button
                    className="rounded border px-2 py-1 text-red-600"
                    onClick={() =>
                      void executionClient
                        .cancelExecution(run.id)
                        .catch((error) => onError(workbenchErrorMessage(error)))
                    }
                  >
                    Stop execution
                  </button>
                )}
                {run.adapter === "stata" && stataUsesLauncher(run.command) && (
                  <p>
                    Stopping gives the Stata launcher up to 30 seconds to exit
                    on its own before it is forced to stop.
                  </p>
                )}
                {run.validation.stataCleanupRequired === true && (
                  <p role="alert" className="text-danger">
                    {STATA_CLEANUP_NOTICE}
                  </p>
                )}
                {openRuns.has(run.id) && (
                  <RunOutput workspaceId={workspaceId} run={run} />
                )}
                <pre className="overflow-auto whitespace-pre-wrap">
                  {JSON.stringify(run.validation, null, 2)}
                </pre>
              </div>
            </details>
          ))}
        </div>
        <p className="mt-2 text-[10px] text-gray-500">
          These checks compare exported numbers. Assess identification, causal
          interpretation, and model validity separately.
        </p>
      </section>
    </div>
  );
}
