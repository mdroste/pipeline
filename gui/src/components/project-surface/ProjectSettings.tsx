import { documentType } from "../../lib/documentTypes";
import { currentManuscript } from "../../lib/project-overview/manuscript";
import { useEffect, useRef, useState } from "react";
import useDraftText from "../../hooks/useDraftText";
import {
  remoteLabel,
  remoteSummary,
  repositoryClient,
  type RepositoryStatus,
} from "../../lib/repositoryClient";
import "./ProjectHome.css";
import EarlierRecords from "./EarlierRecords";
import ProjectConnections from "./ProjectConnections";
import ProjectSnapshots from "./ProjectSnapshots";
import ProjectChecks from "./ProjectChecks";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { projectKinds, projectStages } from "../../lib/workspaceNavigation";
import {
  Card,
  button,
  input,
  muted,
  notice,
  earlierVersionLabel,
  versionLabel,
  formatDate,
  type SurfaceApi,
} from "./shared";

// A date input reports a complete value for every digit typed into the year
// (2 becomes 0002), so only a four-digit year is ever saved.
const isTargetDate = (text: string) =>
  /^\d{4}-\d{2}-\d{2}$/.test(text) && Number(text.slice(0, 4)) >= 1000;

export type ProjectSettingsSection =
  | "publication"
  | "documents"
  | "folder"
  | "exclusions"
  | "repository"
  | "records";

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
  section,
  ...api
}: SurfaceApi & {
  watch: boolean;
  setWatch: (value: boolean) => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
  /** The overview embeds one bounded section; the full route retains all. */
  section?: ProjectSettingsSection;
}) {
  const show = (name: ProjectSettingsSection) => !section || section === name;
  const [ignoreDraft, setIgnoreDraft] = useState(() =>
    data.settings.body.ignoredPaths.join("\n"),
  );
  const [targetLabel, setTargetLabel] = useState(
    () => data.settings.body.targetLabel ?? "",
  );
  const [venue, setVenue] = useState(() => data.settings.body.venue ?? "");
  const previousSettings = useRef(data.settings.body);
  useEffect(() => {
    // Options and the full settings route may both be retained. Refresh fields
    // that still match the last saved value, without replacing a local draft.
    const previous = previousSettings.current;
    const current = data.settings.body;
    setVenue((draft) =>
      draft.trim() === (previous.venue ?? "") ? (current.venue ?? "") : draft,
    );
    setTargetLabel((draft) =>
      draft.trim() === (previous.targetLabel ?? "")
        ? (current.targetLabel ?? "")
        : draft,
    );
    setIgnoreDraft((draft) =>
      draft
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter(Boolean)
        .join("\n") === previous.ignoredPaths.join("\n")
        ? current.ignoredPaths.join("\n")
        : draft,
    );
    previousSettings.current = current;
  }, [data.settings.body]);
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
  const versions = data.papers.filter(
    (p) => p.revision && documentType(p.paper) === "paper" && !p.paper.archived,
  );
  const mainVersion = data.settings.body.manuscriptRevisionId;
  const earlier =
    mainVersion && !data.papers.some((p) => p.revision?.id === mainVersion)
      ? [mainVersion]
      : [];
  const inventory = data.inventory?.body;
  const unreadable = inventory?.files.filter((f) => !f.hash) ?? [];
  return (
    <div
      className={section ? "overview-settings" : "mx-auto max-w-6xl space-y-5"}
      data-section={section}
    >
      {!section && <h2 className="text-lg font-semibold">Project settings</h2>}
      <div className="workspace-overview-details-body">
        {show("documents") && (
          <div className="mb-4 flex flex-wrap gap-2">
            <button className={button} onClick={() => onImportPaper(false)}>
              Add document
            </button>
            <button className={button} onClick={() => onImportPaper(true)}>
              Import LaTeX folder
            </button>
          </div>
        )}
        <div className="grid gap-5 lg:grid-cols-2">
          {show("repository") && (
            <ProjectConnections
              {...api}
              workspaceId={workspaceId}
              data={data}
              workspace={workspace}
              act={act}
              saveSettings={saveSettings}
              reportError={reportError}
              repository={repository}
            />
          )}
          {show("documents") && (
            <Card title="Main paper">
              {!versions.length && (
                <p className={muted}>
                  No paper selected. Projects can contain only slides or other
                  documents.
                </p>
              )}
              <label className="block text-xs">
                Primary manuscript
                <select
                  aria-label="Primary manuscript"
                  className={`${input} mt-1`}
                  value={
                    currentManuscript(data).chosen
                      ? currentManuscript(data).paper?.paper.id
                      : ""
                  }
                  onChange={(e) =>
                    void saveSettings({
                      primaryDocumentId: e.target.value || null,
                    })
                  }
                >
                  <option value="">Not chosen</option>
                  {versions.map((p) => (
                    <option key={p.paper.id} value={p.paper.id}>
                      {p.paper.title}
                    </option>
                  ))}
                </select>
              </label>
              <label className="block text-xs">
                Reference version
                <select
                  aria-label="Reference version of the paper"
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
                The manuscript follows its latest saved version. The reference
                stays fixed until you choose another; working files, builds, and
                reviews keep their own versions.
              </p>
            </Card>
          )}

          {show("publication") && (
            <Card title="Publication details (optional)">
              <p className={muted}>
                Optional publication metadata for this project.
              </p>
              <div className="grid gap-3 sm:grid-cols-2">
                <label className="block text-xs">
                  Kind
                  <select
                    aria-label="Kind of paper"
                    className={`${input} mt-1`}
                    value={data.settings.body.kind ?? ""}
                    onChange={(e) =>
                      void saveSettings({ kind: e.target.value || null })
                    }
                  >
                    <option value="">Not set</option>
                    {projectKinds.map((kind) => (
                      <option key={kind.id} value={kind.id}>
                        {kind.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="block text-xs">
                  Stage
                  <select
                    aria-label="Stage of the paper"
                    className={`${input} mt-1`}
                    value={data.settings.body.stage ?? ""}
                    onChange={(e) =>
                      void saveSettings({ stage: e.target.value || null })
                    }
                  >
                    <option value="">Not set</option>
                    {projectStages.map((stage) => (
                      <option key={stage.id} value={stage.id}>
                        {stage.label}
                      </option>
                    ))}
                  </select>
                </label>
              </div>
              <label className="block text-xs">
                Venue
                <input
                  type="text"
                  aria-label="Venue"
                  className={`${input} mt-1`}
                  value={venue}
                  maxLength={120}
                  placeholder="Journal of Labor Economics"
                  onChange={(e) => setVenue(e.target.value)}
                  onBlur={() => {
                    if (venue.trim() !== (data.settings.body.venue ?? ""))
                      void saveSettings({ venue: venue.trim() });
                  }}
                />
              </label>
            </Card>
          )}

          {show("repository") && repository && (
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
                  ? remoteSummary(repository.remote)
                  : "Local Git repository with no remote"}
                {repository.branch ? ` · branch ${repository.branch}` : ""}
              </p>
              <p className={muted}>
                {repository.upstream
                  ? repository.fetchedAt
                    ? `${repository.ahead} to push, ${repository.behind} to pull, as of ${formatDate(repository.fetchedAt)}.`
                    : `${repository.ahead} to push and ${repository.behind} to pull. Not checked against the remote yet.`
                  : "This branch does not track a remote branch."}{" "}
                {repository.changed + repository.untracked > 0 &&
                  `${repository.changed} uncommitted and ${repository.untracked} untracked files.`}
              </p>
              <p className={muted}>
                This comes from the folder&rsquo;s Git data. The Check button
                runs <code>git fetch</code> with your Git credentials. It
                doesn&rsquo;t commit, push, or merge.
              </p>
            </Card>
          )}

          {section === "repository" && !repository && (
            <p className={muted}>
              No repository status available. Attach a Git project folder to see
              its local and remote state.
            </p>
          )}
          {show("publication") && (
            <Card title="Target date">
              <p className={muted}>
                Shows at the top of the project overview. Use it for a deadline,
                such as a submission or resubmission.
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
                      if (
                        targetLabel !== (data.settings.body.targetLabel ?? "")
                      )
                        void saveSettings({ targetLabel: targetLabel.trim() });
                    }}
                  />
                </label>
              </div>
            </Card>
          )}

          {(show("folder") || show("exclusions")) && (
            <Card
              title={section === "exclusions" ? "Folder exclusions" : "Folder"}
              action={
                <button className={button} onClick={onAttachFolder}>
                  {workspace?.root ? "Change folder" : "Attach folder"}
                </button>
              }
            >
              <div hidden={section === "exclusions"}>
                {workspace?.root ? (
                  <p className="break-all font-mono text-xs">
                    {workspace.root}
                  </p>
                ) : (
                  <p className={muted}>
                    This project was saved without a folder. Attach its research
                    folder to browse files and continue working. Your saved
                    documents, notes, and conversations stay here.
                  </p>
                )}
                {workspace?.missingRootAt && (
                  <p className={notice}>
                    This folder could not be found. It may have been moved or is
                    on a disk that is not connected.
                  </p>
                )}
              </div>
              {workspace?.root && (
                <>
                  <div hidden={section === "exclusions"}>
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
                      <p key={f.path} className="text-xs text-warning">
                        {f.path}: {f.status}
                      </p>
                    ))}
                    {inventory?.warnings.map((w) => (
                      <p key={w} className="text-xs text-warning">
                        {w}
                      </p>
                    ))}
                    {!data.fileAcceptance && (
                      <p className={notice}>
                        On this platform the assistant can edit a working copy,
                        but its edits cannot be copied back into this folder.
                      </p>
                    )}
                  </div>
                  <details
                    hidden={section === "folder"}
                    open={section === "exclusions" || undefined}
                  >
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
          )}

          {show("records") && <ProjectChecks workspaceId={workspaceId} />}
          {show("records") && (
            <ProjectSnapshots workspaceId={workspaceId} data={data} />
          )}
          {show("records") && (
            <EarlierRecords
              workspaceId={workspaceId}
              root={workspace?.root ?? null}
              reportError={reportError}
            />
          )}
        </div>
      </div>
    </div>
  );
}
