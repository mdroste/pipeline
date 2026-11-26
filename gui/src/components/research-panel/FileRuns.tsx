import { resolveFileLink } from "../../lib/fileLinks";
import type { ResearchExecution } from "../../lib/workbench/executionTypes";
import ExecutionEvidence from "./ExecutionEvidence";
import RunOutput from "./RunOutput";

export function fileRunLinks(
  runs: ResearchExecution[],
  root: string | null,
  path: string,
  hash?: string,
) {
  return runs.flatMap((run) => {
    const matches = (manifest: Record<string, unknown>, key: string) =>
      Array.isArray(manifest[key]) &&
      manifest[key].some((file: { path?: string; contentHash?: string }) => {
        const relative = resolveFileLink(
          "",
          encodeURIComponent(`${run.cwd}/${file.path}`),
          root ?? undefined,
        );
        return relative?.path === path && (!hash || file.contentHash === hash);
      });
    return matches(run.outputManifest, "artifacts")
      ? [
          {
            run,
            role: hash
              ? "Produced this file version"
              : "Recorded output at this path; current version not verified",
          },
        ]
      : matches(run.inputManifest, "files")
        ? [
            {
              run,
              role: hash
                ? "Used this file version"
                : "Recorded input at this path; current version not verified",
            },
          ]
        : [];
  });
}

export default function FileRuns({
  workspaceId,
  runs,
  root,
  path,
  hash,
}: {
  workspaceId: string;
  runs: ResearchExecution[];
  root: string | null;
  path: string;
  hash?: string;
}) {
  const links = fileRunLinks(runs, root, path, hash);
  return (
    <div className="space-y-3">
      <p className="text-xs">
        {path} ·{" "}
        {hash ? `version ${hash.slice(0, 12)}` : "file identity unavailable"}
      </p>
      {!links.length && (
        <p>
          No matching run in the loaded history. This does not establish how the
          file was produced.
        </p>
      )}
      {runs.length >= 200 && (
        <p className="text-xs">History is limited to the latest 200 runs.</p>
      )}
      {links.map(({ run, role }) => (
        <details key={run.id}>
          <summary>
            {role} · {run.outcome} · {run.command.join(" ")}
          </summary>
          <RunOutput workspaceId={workspaceId} run={run} />
          <ExecutionEvidence workspaceId={workspaceId} run={run} />
        </details>
      ))}
    </div>
  );
}
