import { useEffect, useRef, useState } from "react";
import useDraftText from "../../hooks/useDraftText";
import {
  remoteLabel,
  repositoryClient,
  type RepositoryStatus,
} from "../../lib/repositoryClient";
import "./ProjectHome.css";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import {
  Card,
  button,
  input,
  muted,
  notice,
  earlierVersions,
  earlierVersionLabel,
  versionLabel,
  runLabel,
  formatDate,
  type SurfaceApi,
} from "./shared";

// A date input reports a complete value for every digit typed into the year
// (2 becomes 0002), so only a four-digit year is ever saved.
const isTargetDate = (text: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(text) && Number(text.slice(0, 4)) >= 1000;

export default function ProjectSettings({
  data,
  workspace,
  act,
  saveSettings,
  workspaceId,
  reportError,
  watch,
  setWatch,
  onImportPaper,
  onAttachFolder,
}: SurfaceApi & {
  watch: boolean;
  setWatch: (value: boolean) => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
}) {
  const [ignoreDraft, setIgnoreDraft] = useState(() =>
    data.settings.body.ignoredPaths.join("\n"),
  );
  const ignoreSeeded = useRef(false);
  useEffect(() => {
    if (!ignoreSeeded.current) {
      ignoreSeeded.current = true;
      setIgnoreDraft(data.settings.body.ignoredPaths.join("\n"));
    }
  }, [data.settings.body.ignoredPaths]);

  const [targetLabel, setTargetLabel] = useState(
    () => data.settings.body.targetLabel ?? "",
  );
  // Saved on blur, Enter, or a short pause on a complete date, so a two-digit
  // month or day is never saved half-typed.
  const savedDate = data.settings.body.targetDate ?? "";
  const partialDate = useRef(false);
  const commitDate = (text: string) => {
    if (text === savedDate) return;
    // Empty with some segments still filled is unfinished, not a clear.
    if (text ? isTargetDate(text) : !partialDate.current)
      void saveSettings({ targetDate: text || null });
  };
  const targetDate = useDraftText(savedDate, commitDate);
  useEffect(() => {
    if (targetDate.value === savedDate || !isTargetDate(targetDate.value))
      return;
    const timer = setTimeout(() => commitDate(targetDate.value), 800);
    return () => clearTimeout(timer);
  }, [targetDate.value, savedDate, saveSettings]);
  // Local Git state of the attached folder; null when it is not a repository.
  const [repository, setRepository] = useState<RepositoryStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const root = workspace?.root ?? null;
  useEffect(() => {
    let stale = false;
    setRepository(null);
    if (root)
      void Promise.resolve()
        .then(() => repositoryClient.status(workspaceId))
        .then((status) => {
          if (!stale) setRepository(status);
        })
        .catch(() => undefined);
    return () => {
      stale = true;
    };
  }, [workspaceId, root]);
  const checkRemote = async () => {
    setChecking(true);
    try {
      setRepository(await repositoryClient.fetch(workspaceId));
    } catch (cause) {
      reportError(workbenchErrorMessage(cause));
    } finally {
      setChecking(false);
    }
  };
  const versions = data.papers.filter((p) => p.revision);
  const earlier = earlierVersions(data);
  const completedRuns = data.executions.filter(
    (r) => r.outcome === "completed",
  );
  const inventory = data.inventory?.body;
  const unreadable = inventory?.files.filter((f) => !f.hash) ?? [];
  return (
    <div className="mx-auto max-w-6xl space-y-5">
      <h2 className="text-lg font-semibold">Project settings</h2>
      <div className="workspace-overview-details-body">
        <div className="mb-4 flex flex-wrap gap-2">
          <button className={button} onClick={() => onImportPaper(false)}>
            Import paper
          </button>
          <button className={button} onClick={() => onImportPaper(true)}>
            Import LaTeX folder
          </button>
        </div>
        <div className="grid gap-5 lg:grid-cols-2">
          <Card title="Paper">
            {!versions.length && (
              <p className={muted}>No paper imported yet.</p>
            )}
            <label className="block text-xs">
              Current version
              <select
                aria-label="Current version of the paper"
                className={`${input} mt-1`}
                value={data.settings.body.manuscriptRevisionId ?? ""}
                onChange={(e) =>
                  void saveSettings({
                    manuscriptRevisionId: e.target.value || null,
                  })
                }
              >
                <option value="">Not chosen</option>
                {versions.map((p) => (
                  <option key={p.revision!.id} value={p.revision!.id}>
                    {versionLabel(p)}
                  </option>
                ))}
                {earlier.map((id) => (
                  <option key={id} value={id}>
                    {earlierVersionLabel(id)}
                  </option>
                ))}
              </select>
            </label>
            <p className={muted}>
              Notes, action items, and comparisons refer to this version.
              Importing a newer file does not change the choice until you make
              it here.
            </p>
            <label className="block text-xs">
              Reference results
              <select
                aria-label="Reference results run"
                className={`${input} mt-1`}
                value={data.settings.body.baselineExecutionId ?? ""}
                onChange={(e) =>
                  void saveSettings({
                    baselineExecutionId: e.target.value || null,
                  })
                }
              >
                <option value="">
                  {completedRuns.length
                    ? "Not chosen"
                    : "No completed runs yet"}
                </option>
                {completedRuns.map((r) => (
                  <option key={r.id} value={r.id}>
                    {runLabel(r)}
                  </option>
                ))}
              </select>
            </label>
            <p className={muted}>
              Choose the completed experiment to use as the baseline for new
              results.
            </p>
            {data.ledger.staleClaims.length > 0 && (
              <p className="text-xs text-amber-700 dark:text-amber-300">
                {data.ledger.staleClaims.length === 1
                  ? "One claim rests on evidence that is out of date."
                  : `${data.ledger.staleClaims.length} claims rest on evidence that is out of date.`}
              </p>
            )}
          </Card>

          {repository && (
            <Card
              title="Repository"
              action={
                repository.remote && (
                  <button
                    className={button}
                    disabled={checking}
                    onClick={() => void checkRemote()}
                  >
                    {checking
                      ? "Checking…"
                      : `Check ${remoteLabel(repository.remote)} now`}
                  </button>
                )
              }
            >
              <p className="text-sm">
                {repository.remote
                  ? `${repository.remote.owner ? `${repository.remote.owner}/` : ""}${repository.remote.repo} on ${remoteLabel(repository.remote)}`
                  : "Local Git repository with no remote"}
                {repository.branch ? ` · branch ${repository.branch}` : ""}
              </p>
              <p className={muted}>
                {repository.upstream
                  ? `${repository.ahead} to push, ${repository.behind} to pull, as of ${repository.fetchedAt ? formatDate(repository.fetchedAt) : "a check that has not happened yet"}.`
                  : "This branch does not track a remote branch."}{" "}
                {repository.changed + repository.untracked > 0 &&
                  `${repository.changed} uncommitted and ${repository.untracked} untracked files.`}
              </p>
              <p className={muted}>
                The overview reads this from the folder&rsquo;s own Git data.
                Checking the remote runs <code>git fetch</code> with your Git
                credentials and happens only when you ask; nothing is committed,
                pushed, or merged.
              </p>
            </Card>
          )}

          <Card title="Target date">
            <p className={muted}>
              One optional date to keep in view, such as a submission or
              resubmission deadline. It appears at the top of the project
              overview.
            </p>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block text-xs">
                Date
                <input
                  type="date"
                  aria-label="Target date"
                  className={`${input} mt-1`}
                  {...targetDate}
                  onChange={(e) => {
                    partialDate.current = e.target.validity.badInput;
                    targetDate.onChange(e);
                  }}
                />
              </label>
              <label className="block text-xs">
                What it is
                <input
                  type="text"
                  aria-label="Target label"
                  className={`${input} mt-1`}
                  value={targetLabel}
                  maxLength={80}
                  placeholder="Submission"
                  onChange={(e) => setTargetLabel(e.target.value)}
                  onBlur={() => {
                    if (targetLabel !== (data.settings.body.targetLabel ?? ""))
                      void saveSettings({ targetLabel: targetLabel.trim() });
                  }}
                />
              </label>
            </div>
          </Card>

          <Card
            title="Folder"
            action={
              <button className={button} onClick={onAttachFolder}>
                {workspace?.root ? "Change folder" : "Attach folder"}
              </button>
            }
          >
            {workspace?.root ? (
              <p className="break-all font-mono text-xs">{workspace.root}</p>
            ) : (
              <p className={muted}>
                No folder attached. Attach the folder that holds the paper and
                its code to list its files and let the assistant propose edits
                in a separate working copy.
              </p>
            )}
            {workspace?.missingRootAt && (
              <p className={notice}>
                This folder could not be found. It may have been moved or is on
                a disk that is not connected.
              </p>
            )}
            {workspace?.root && (
              <>
                <div className="flex flex-wrap items-center gap-3">
                  <button
                    className={button}
                    onClick={() => void act({ action: "refresh" })}
                  >
                    Refresh file list
                  </button>
                  <label className={`flex items-center gap-1.5 ${muted}`}>
                    <input
                      type="checkbox"
                      checked={watch}
                      onChange={(e) => setWatch(e.target.checked)}
                    />
                    Check for changes every 30 seconds
                  </label>
                </div>
                <p className={muted}>{data.workingCopyStatus}</p>
                {inventory && (
                  <p className={muted}>
                    {inventory.files.length} files listed, last refreshed{" "}
                    {formatDate(inventory.capturedAt)}.
                    {!inventory.complete &&
                      " Some files could not be read or were skipped."}
                  </p>
                )}
                {unreadable.slice(0, 10).map((f) => (
                  <p
                    key={f.path}
                    className="text-xs text-amber-700 dark:text-amber-300"
                  >
                    {f.path}: {f.status}
                  </p>
                ))}
                {inventory?.warnings.map((w) => (
                  <p
                    key={w}
                    className="text-xs text-amber-700 dark:text-amber-300"
                  >
                    {w}
                  </p>
                ))}
                {!data.fileAcceptance && (
                  <p className={notice}>
                    On this platform the assistant can edit a working copy, but
                    its edits cannot be copied back into this folder.
                  </p>
                )}
                <details>
                  <summary className={`cursor-pointer ${muted}`}>
                    Folders and files to leave out
                  </summary>
                  <textarea
                    aria-label="Paths to leave out"
                    className={`${input} mt-2`}
                    value={ignoreDraft}
                    onChange={(e) => setIgnoreDraft(e.target.value)}
                    rows={3}
                    placeholder="One path per line, relative to the folder"
                  />
                  <p className={`my-2 ${muted}`}>
                    Build, cache, and version-control folders are always left
                    out.
                  </p>
                  <button
                    className={button}
                    onClick={() =>
                      void saveSettings({
                        ignoredPaths: ignoreDraft
                          .split("\n")
                          .map((p) => p.trim())
                          .filter(Boolean),
                      })
                    }
                  >
                    Save
                  </button>
                </details>
              </>
            )}
          </Card>
        </div>
      </div>
    </div>
  );
}
