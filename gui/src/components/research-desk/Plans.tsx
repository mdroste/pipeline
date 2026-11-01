import { useCallback, useEffect, useState } from "react";
import {
  deskClient,
  reference,
  type DeskRecord,
  type PlanStatus,
} from "../../lib/deskClient";
import { executionClient } from "../../lib/workbench/executionClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { ExecutionProfile } from "../../lib/workbenchTypes";
import useReopenCount from "../../hooks/useReopenCount";
import useReveal from "../../hooks/useReveal";
import Jobs from "../research-studio/Jobs";
import { button, input, card, muted, type DeskProps } from "./shared";
export default function Plans({
  workspaceId,
  onError,
  onChanged,
  onActionStart,
  active = true,
}: DeskProps) {
  const [inputs, setInputs] = useState<DeskRecord[]>([]);
  const [selectedInputs, setSelectedInputs] = useState<string[]>([]);
  const [profiles, setProfiles] = useState<ExecutionProfile[]>([]);
  const [plans, setPlans] = useState<DeskRecord[]>([]);
  const [profileId, setProfileId] = useState("");
  const [parameters, setParameters] = useState("{}");
  const [seed, setSeed] = useState("");
  const [version, setVersion] = useState("");
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<PlanStatus | null>(null);
  const refresh = useCallback(async () => {
    const [p, c] = await Promise.all([
      executionClient.listExecutionProfiles(workspaceId),
      deskClient.records(workspaceId, "execution_plan"),
    ]);
    setProfiles(p);
    setPlans(c);
    setInputs([
      ...(await deskClient.records(workspaceId, "dataset")),
      ...(await deskClient.records(workspaceId, "sample")),
    ]);
  }, [workspaceId]);
  const reopened = useReopenCount(active);
  const planReview = useReveal<HTMLElement>();
  const missing = [
    !profileId && "choose a configured profile",
    !version.trim() && "enter the toolchain version",
  ].filter(Boolean);
  useEffect(() => {
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [refresh, onError, reopened]);
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    onActionStart?.();
    try {
      await fn();
      onChanged?.();
      await refresh();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const queue = (testOnly: boolean) =>
    run(async () => {
      if (!preview) return;
      const profile = preview.record.body.profile as ExecutionProfile;
      await executionClient.runExecution({
        profileId: profile.id,
        planId: preview.record.id,
        sessionId: null,
        testOnly,
        operationId: `plan-run-${crypto.randomUUID()}`,
      });
    });
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Saved analysis plans</h2>
        <p className={muted}>
          Save a command’s scripts and listed input files. Each run uses a new
          working folder with those saved files. The command still has your
          computer’s file and network permissions.
        </p>
        <select
          aria-label="Execution profile to capture"
          className={input}
          value={profileId}
          onChange={(e) => setProfileId(e.target.value)}
        >
          <option value="">Choose a configured profile</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <p className={muted}>
          Set up profiles in Analyze → Command settings. Use relative script
          paths and declare every input and package lockfile.
        </p>
        <fieldset className="space-y-1">
          <legend className="text-xs">
            Exact datasets and declared sample (optional)
          </legend>
          {inputs.map((i) => (
            <label className="flex gap-2 text-xs" key={i.id}>
              <input
                type="checkbox"
                checked={selectedInputs.includes(i.id)}
                onChange={(e) =>
                  setSelectedInputs((old) =>
                    e.target.checked
                      ? [...old, i.id]
                      : old.filter((id) => id !== i.id),
                  )
                }
              />
              {i.kind}: {i.title} · {i.contentHash.slice(0, 8)}
            </label>
          ))}
        </fieldset>
        <textarea
          aria-label="Execution parameters"
          className={input}
          value={parameters}
          onChange={(e) => setParameters(e.target.value)}
          placeholder="JSON parameters"
        />
        <input
          aria-label="Random seed"
          className={input}
          value={seed}
          onChange={(e) => setSeed(e.target.value)}
          placeholder="Optional seed, exposed as PIPELINE_RANDOM_SEED"
        />
        <input
          aria-label="Toolchain version"
          className={input}
          value={version}
          onChange={(e) => setVersion(e.target.value)}
          placeholder="Declared toolchain version or environment description"
        />
        <button
          className={button}
          disabled={busy || !profileId || !version.trim()}
          onClick={() =>
            void run(async () => {
              const r = await deskClient.capture(
                profileId,
                JSON.parse(parameters) as Record<string, unknown>,
                seed || null,
                version,
                inputs
                  .filter((i) => selectedInputs.includes(i.id))
                  .map(reference),
              );
              setPreview(await deskClient.planStatus(workspaceId, r.id));
              planReview.reveal();
            })
          }
        >
          Capture inputs and review plan
        </button>
        {missing.length > 0 && (
          <p className={muted}>To capture a plan, {missing.join(" and ")}.</p>
        )}
      </section>
      {plans.map((p) => (
        <button
          key={p.id}
          className={`${card} block w-full text-left`}
          onClick={() =>
            void run(async () => {
              setPreview(await deskClient.planStatus(workspaceId, p.id));
              planReview.reveal();
            })
          }
        >
          <span className="font-semibold">{p.title}</span>
          <p className={muted}>
            {new Date(p.createdAt).toLocaleString()} ·{" "}
            {p.contentHash.slice(0, 12)}
          </p>
        </button>
      ))}
      {preview && (
        <section ref={planReview.ref} tabIndex={-1} className={card}>
          <h2 className="font-semibold">Review exact plan</h2>
          <p className="text-sm">
            This command runs with your computer’s permissions. It can access
            files and the network beyond the saved inputs.
          </p>
          <pre className="max-h-96 overflow-auto whitespace-pre-wrap text-xs">
            {JSON.stringify(preview.record.body, null, 2)}
          </pre>
          <p className={muted}>
            Authorization:{" "}
            {preview.authorized ? "granted for this fingerprint" : "required"} ·
            Captured-plan test: {preview.testStatus ?? "not tested"}
          </p>
          {!preview.authorized && (
            <button
              className={button}
              disabled={busy}
              onClick={() =>
                void run(async () =>
                  setPreview(
                    await deskClient.authorizePlan(
                      workspaceId,
                      preview.record.id,
                      preview.record.contentHash,
                    ),
                  ),
                )
              }
            >
              Allow this plan to run on this computer
            </button>
          )}
          <button
            className={button}
            disabled={busy || !preview.authorized}
            onClick={() => void queue(true)}
          >
            Test saved plan
          </button>
          <button
            className={button}
            disabled={
              busy || !preview.authorized || preview.testStatus !== "passed"
            }
            onClick={() => void queue(false)}
          >
            Run in a new working folder
          </button>
        </section>
      )}
      <Jobs
        workspaceId={workspaceId}
        visible={active}
        onCompleted={() => {
          onChanged?.();
          void refresh();
          if (preview)
            void deskClient
              .planStatus(workspaceId, preview.record.id)
              .then(setPreview)
              .catch((e) => onError(workbenchErrorMessage(e)));
        }}
      />
    </div>
  );
}
