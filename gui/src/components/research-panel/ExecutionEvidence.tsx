import { useState } from "react";
import { projectClient } from "../../lib/projectClient";
import { appEvents } from "../../lib/appEvents";
import { declaredFileHashes } from "../../lib/analysisSetup";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import type { ResearchExecution } from "../../lib/workbench/executionTypes";
import { button } from "../../ui/classes";

export default function ExecutionEvidence({
  workspaceId,
  run,
}: {
  workspaceId: string;
  run: ResearchExecution;
}) {
  const [drift, setDrift] = useState("");
  const [busy, setBusy] = useState(false);
  const artifacts = Array.isArray(run.outputManifest.artifacts)
    ? (run.outputManifest.artifacts as {
        path: string;
        artifactId: string;
        contentHash: string;
      }[])
    : [];
  const check = async () => {
    setBusy(true);
    try {
      if (!run.profileId)
        throw new Error(
          "This receipt has no local execution profile; current inputs cannot be checked here.",
        );
      const current = await projectClient.previewExecution(run.profileId);
      const changed =
        declaredFileHashes(run.inputManifest) !==
        declaredFileHashes(current.inputs);
      const exact =
        run.validation.authorizationFingerprint === current.fingerprint;
      setDrift(
        exact
          ? "The recorded command, declared inputs, and recorded environment still match. Undeclared dependencies remain unknown."
          : changed
            ? "Declared inputs changed or became unavailable since this run. Inspect the manifest before rerunning."
            : "The command profile or recorded environment differs, or this older receipt lacks its authorization identity. Its output is not a check of the current setup.",
      );
    } catch (cause) {
      setDrift(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="space-y-2 text-xs">
      <p>
        Recorded inputs and outputs only. External data, undeclared packages,
        and hidden notebook state may be unknown.
      </p>
      <button className={button} disabled={busy} onClick={() => void check()}>
        {busy ? "Checking inputs…" : "Compare with current inputs"}
      </button>
      {drift && <p role="status">{drift}</p>}
      {artifacts.length > 0 && (
        <ul>
          {artifacts.map((artifact) => (
            <li key={artifact.artifactId}>
              <button
                className="underline"
                onClick={() =>
                  appEvents.emit("open-object", {
                    workspaceId,
                    object: {
                      kind: "artifact",
                      id: artifact.artifactId,
                      revision: artifact.contentHash,
                    },
                  })
                }
              >
                {artifact.path} · saved output{" "}
                {artifact.contentHash.slice(0, 8)}
              </button>
            </li>
          ))}
        </ul>
      )}
      <details>
        <summary>Input, environment, and output manifest</summary>
        <pre className="max-h-80 overflow-auto whitespace-pre-wrap">
          {JSON.stringify(
            {
              command: run.command,
              cwd: run.cwd,
              inputs: run.inputManifest,
              environment: run.validation.authorizedLaunch,
              runtime: run.validation.runtimeIdentity,
              outputs: run.outputManifest,
            },
            null,
            2,
          )}
        </pre>
      </details>
    </div>
  );
}
