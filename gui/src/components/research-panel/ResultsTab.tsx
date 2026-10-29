import { projectClient } from "../../lib/projectClient";
import { executionClient } from "../../lib/workbench/executionClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { operation } from "./useResearchPanelController";
import { compactHash } from "./format";
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
  return (
    <div className="space-y-4">
      <section hidden={resultsView === "runs"} className="space-y-2">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">
          New execution profile
        </h3>
        <input
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
          value={inputs}
          onChange={(event) => setInputs(event.target.value)}
          rows={2}
          placeholder="Input paths, one relative path per line"
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <textarea
          value={outputs}
          onChange={(event) => setOutputs(event.target.value)}
          rows={2}
          placeholder="Expected output paths, one per line"
          className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
        />
        <button
          type="button"
          disabled={busy || !workspace?.root}
          onClick={() =>
            void act(async () => {
              const parsed = JSON.parse(argv);
              if (
                !Array.isArray(parsed) ||
                !parsed.every((value) => typeof value === "string")
              )
                throw new Error(
                  "Enter command arguments as a JSON array of text values.",
                );
              await executionClient.saveExecutionProfile({
                profileId: null,
                workspaceId,
                name: profileName,
                adapter,
                argv: parsed,
                cwd: workspace!.root!,
                environment: {},
                inputs: lines(inputs),
                timeoutSeconds: 300,
                outputs: lines(outputs),
                expectedRevision: null,
                operationId: operation("profile"),
              });
            })
          }
          className="rounded border px-3 py-2 text-xs"
        >
          Save profile
        </button>
        {!workspace?.root && (
          <p className="text-xs text-amber-700">
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
              <div className="mt-2 flex items-center gap-2">
                <span
                  className={`text-[11px] ${profile.testStatus === "passed" ? "text-emerald-600" : "text-gray-500"}`}
                >
                  {profile.testStatus ?? "untested"}
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
            >
              <summary className="cursor-pointer text-xs">
                <span
                  className={
                    run.outcome === "completed"
                      ? "text-emerald-600"
                      : run.outcome === "running"
                        ? "text-blue-600"
                        : "text-red-600"
                  }
                >
                  {run.outcome}
                </span>{" "}
                · {run.adapter} · {compactHash(run.dependencyHash)}
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
                  <p role="alert" className="text-red-600">
                    {STATA_CLEANUP_NOTICE}
                  </p>
                )}
                <RunOutput run={run} />
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
