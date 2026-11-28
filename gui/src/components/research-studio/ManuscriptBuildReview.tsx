import { displayPath } from "../../lib/displayPath";
// The paper's build approval uses the same exact-command authorization as Jobs,
// with the manuscript's action and settings kept separate from run diagnostics.
import { useEffect, useState } from "react";
import {
  projectClient,
  type HostExecutionPreview,
} from "../../lib/projectClient";
import { executionClient } from "../../lib/workbench/executionClient";
import type {
  ExecutionProfile,
  ResearchExecution,
} from "../../lib/workbench/executionTypes";
import { ErrorNotice, Inspect, useAction } from "./shared";

export default function ManuscriptBuildReview({
  profile,
  onQueued,
}: {
  profile: ExecutionProfile;
  onQueued: (job: ResearchExecution) => void;
}) {
  const [preview, setPreview] = useState<HostExecutionPreview | null>(null);
  const { error, busy, run } = useAction();
  const review = () =>
    run(async () => {
      setPreview(null);
      setPreview(await projectClient.previewExecution(profile.id));
    });
  useEffect(() => {
    void review();
  }, [profile.id, profile.revision]);
  const inputs = Array.isArray(preview?.inputs.files)
    ? preview.inputs.files.flatMap((file: unknown) =>
        file &&
        typeof file === "object" &&
        "path" in file &&
        typeof file.path === "string"
          ? [file.path]
          : [],
      )
    : [];
  return (
    <div className="manuscript-build-review">
      <ErrorNotice error={error} />
      {preview ? (
        <>
          <p className="text-sm">
            Build with your installed TeX tools. Keep Pipeline open until it
            finishes.
          </p>
          <pre className="manuscript-command">{preview.command.join(" ")}</pre>
          <p className="text-xs text-ink-muted break-all">
            In {displayPath(preview.cwd)}
          </p>
          <p className="text-xs text-ink-muted">
            This command runs with your computer’s access to files and the
            network.
          </p>
          {inputs.length > 0 && (
            <details className="text-xs">
              <summary>{inputs.length} input files</summary>
              <ul className="manuscript-inputs">
                {inputs.map((path) => (
                  <li key={path}>{path}</li>
                ))}
              </ul>
            </details>
          )}
          <Inspect
            label="Build details"
            value={{
              inputs: preview.inputs,
              launch: preview.launch,
              outputs: preview.outputs,
              authorization: preview.boundary,
            }}
          />
          <div className="flex items-center gap-2">
            <button
              className="file-action primary"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await projectClient.authorizeExecution(
                    profile.id,
                    preview.fingerprint,
                  );
                  const job = await executionClient.runExecution({
                    profileId: profile.id,
                    sessionId: null,
                    testOnly: profile.testStatus !== "passed",
                    operationId: `manuscript-${crypto.randomUUID()}`,
                  });
                  setPreview(null);
                  onQueued(job);
                })
              }
            >
              {busy ? "Starting…" : "Authorize and build"}
            </button>
            <button
              className="file-action"
              disabled={busy}
              onClick={() => void review()}
            >
              Refresh
            </button>
          </div>
        </>
      ) : (
        <button
          className="file-action"
          disabled={busy}
          onClick={() => void review()}
        >
          {busy ? "Preparing build…" : "Review build"}
        </button>
      )}
    </div>
  );
}
