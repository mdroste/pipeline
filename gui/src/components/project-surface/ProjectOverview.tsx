import { useEffect, useRef, useState } from "react";
import { workbenchClient } from "../../lib/workbenchClient";
import { projectClient, type ResearchTask } from "../../lib/projectClient";
import type { ResearchNote } from "../../lib/workbenchTypes";
import {
  Card,
  NOTE_KINDS,
  TASK_STATUSES,
  button,
  earlierVersionLabel,
  earlierVersions,
  formatDate,
  input,
  isOpenTask,
  linkButton,
  muted,
  noteKindLabel,
  notice,
  op,
  primaryButton,
  runLabel,
  versionLabel,
  type SurfaceApi,
} from "./shared";

interface Props extends SurfaceApi {
  tasksOnly?: boolean;
  watch: boolean;
  setWatch: (value: boolean) => void;
  onContinue: () => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
}

function NoteCard({ note, api }: { note: ResearchNote; api: SurfaceApi }) {
  const { data, run, saveSettings } = api;
  const brief = data.settings.body.briefNoteIds;
  const excluded = data.settings.body.excludedNoteIds;
  const update = (patch: { state?: "accepted" | "rejected" }) =>
    run(() =>
      workbenchClient.updateNote({
        noteId: note.id,
        expectedRevision: note.revision,
        operationId: op(),
        ...patch,
      }),
    );
  return (
    <div className="rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950">
      <div className={`flex flex-wrap items-center gap-x-2 ${muted}`}>
        <span className="font-medium">{noteKindLabel(note.kind)}</span>
        {note.state === "proposed" && (
          <span>
            · Suggested by {note.origin === "user" ? "you" : "the assistant"},
            not yet accepted
          </span>
        )}
        {note.state === "rejected" && <span>· Rejected</span>}
        {note.state === "retired" && <span>· No longer current</span>}
        <span className="ml-auto">{formatDate(note.updatedAt)}</span>
      </div>
      <p className="my-2 whitespace-pre-wrap text-sm">{note.body}</p>
      {note.state === "proposed" && (
        <div className="flex gap-2">
          <button
            className={button}
            onClick={() => void update({ state: "accepted" })}
          >
            Accept
          </button>
          <button
            className={button}
            onClick={() => void update({ state: "rejected" })}
          >
            Reject
          </button>
        </div>
      )}
      {note.state === "accepted" && (
        <div className={`flex flex-wrap gap-4 ${muted}`}>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={brief.includes(note.id)}
              onChange={(e) =>
                void saveSettings({
                  briefNoteIds: e.target.checked
                    ? [...brief, note.id]
                    : brief.filter((id) => id !== note.id),
                })
              }
            />
            Show in project summary
          </label>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={excluded.includes(note.id)}
              onChange={(e) =>
                void saveSettings({
                  excludedNoteIds: e.target.checked
                    ? [...excluded, note.id]
                    : excluded.filter((id) => id !== note.id),
                })
              }
            />
            Hide from the assistant
          </label>
        </div>
      )}
    </div>
  );
}

export default function ProjectOverview(props: Props) {
  const {
    data,
    workspace,
    workspaceId,
    act,
    run,
    saveSettings,
    openAnnotation,
    setTab,
    setTaskId,
    watch,
    setWatch,
    onContinue,
    onImportPaper,
    onAttachFolder,
  } = props;
  const [noteBody, setNoteBody] = useState("");
  const [noteKind, setNoteKind] = useState<ResearchNote["kind"]>("question");
  const [objective, setObjective] = useState("");
  const [expectedOutputs, setExpectedOutputs] = useState("");
  const [expectedChecks, setExpectedChecks] = useState("");
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

  const versions = data.papers.filter((p) => p.revision);
  const earlier = earlierVersions(data);
  const completedRuns = data.executions.filter(
    (r) => r.outcome === "completed",
  );
  const summaryNotes = data.notes.filter(
    (n) =>
      data.settings.body.briefNoteIds.includes(n.id) && n.state === "accepted",
  );
  const inventory = data.inventory?.body;
  const unreadable = inventory?.files.filter((f) => !f.hash) ?? [];
  const recovery = data.applications.filter((a) =>
    ["applying", "recovery_required"].includes(a.body.state),
  );
  const proposed = data.notes.filter((n) => n.state === "proposed");
  const canEdit = Boolean(workspace?.root) && data.fileAcceptance;
  const empty = !versions.length && !workspace?.root;
  const openTasks = data.tasks.filter((task) => isOpenTask(task.body.status));
  const currentPaper = versions.find(
    (paper) => paper.revision?.id === data.settings.body.manuscriptRevisionId,
  );

  const tasks = (
    <section className="space-y-3">
      <h2 className="font-semibold">Action items</h2>
      <div className="flex flex-wrap gap-2 sm:flex-nowrap">
        <textarea
          aria-label="New action item"
          value={objective}
          onChange={(e) => setObjective(e.target.value)}
          rows={2}
          className={input}
          placeholder="What needs doing?"
        />
        <button
          className={button}
          disabled={!objective.trim()}
          onClick={() =>
            void run(async () => {
              const task = await projectClient.mutate<ResearchTask>(
                workspaceId,
                {
                  action: "createTask",
                  objective,
                  anchorId: null,
                  expectedOutputs: expectedOutputs
                    .split("\n")
                    .map((s) => s.trim())
                    .filter(Boolean),
                  expectedChecks: expectedChecks
                    .split("\n")
                    .map((s) => s.trim())
                    .filter(Boolean),
                },
              );
              setTaskId(task.id);
              setObjective("");
              setExpectedOutputs("");
              setExpectedChecks("");
            })
          }
        >
          Add action item
        </button>
      </div>
      <details>
        <summary className={`cursor-pointer ${muted}`}>
          Intended outputs and checks (optional)
        </summary>
        <div className="mt-2 grid gap-3 sm:grid-cols-2">
          <textarea
            aria-label="Intended outputs"
            value={expectedOutputs}
            onChange={(e) => setExpectedOutputs(e.target.value)}
            className={input}
            rows={3}
            placeholder="Files or results this action item should produce, one per line"
          />
          <textarea
            aria-label="Checks to run"
            value={expectedChecks}
            onChange={(e) => setExpectedChecks(e.target.value)}
            className={input}
            rows={3}
            placeholder="How to check the result, one per line"
          />
        </div>
      </details>
      {!data.tasks.length && <p className={muted}>No action items yet.</p>}
      {data.tasks.map((t) => (
        <div
          key={t.id}
          className="flex flex-wrap items-center gap-3 rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950"
        >
          <span
            className={`min-w-0 flex-1 text-sm ${isOpenTask(t.body.status) ? "" : "text-gray-500 line-through decoration-gray-300"}`}
          >
            {t.body.objective}
          </span>
          {t.body.anchorId && (
            <button
              className={linkButton}
              onClick={() => {
                const anchor = data.anchors.find(
                  (a) => a.id === t.body.anchorId,
                );
                if (anchor) openAnnotation(anchor);
              }}
            >
              Show passage
            </button>
          )}
          {canEdit && isOpenTask(t.body.status) && (
            <button
              className={linkButton}
              onClick={() => {
                setTaskId(t.id);
                setTab("edits");
              }}
            >
              Start an edit
            </button>
          )}
          <select
            aria-label={`Status: ${t.body.objective}`}
            className="rounded-md border border-gray-300 bg-transparent p-1 text-xs dark:border-neutral-700"
            value={t.body.status}
            onChange={(e) =>
              void act({
                action: "updateTask",
                taskId: t.id,
                expectedRevision: t.revision,
                status: e.target.value,
              })
            }
          >
            {TASK_STATUSES.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>
      ))}
    </section>
  );
  if (props.tasksOnly) return <div className="mx-auto max-w-6xl">{tasks}</div>;

  return (
    <div className="mx-auto max-w-6xl space-y-6">
      <div className="workspace-overview-intro">
        <h2>
          {empty
            ? "Start your project"
            : (currentPaper?.paper.title ?? "Project brief")}
        </h2>
        <p className={muted}>
          {empty
            ? "Add a paper or attach a folder to begin."
            : `${versions.length} document${versions.length === 1 ? "" : "s"} · ${openTasks.length} open action item${openTasks.length === 1 ? "" : "s"}`}
        </p>
        <div className="mt-4 flex flex-wrap gap-2">
          {empty ? (
            <>
              <button
                className={primaryButton}
                onClick={() => onImportPaper(false)}
              >
                Import paper
              </button>
              <button className={button} onClick={onAttachFolder}>
                Attach folder
              </button>
            </>
          ) : (
            <>
              <button
                className={primaryButton}
                onClick={() => setTab(versions.length ? "documents" : "files")}
              >
                {versions.length ? "Open documents" : "Open files"}
              </button>
              <button className={button} onClick={onContinue}>
                Continue in a conversation
              </button>
            </>
          )}
        </div>
      </div>
      {recovery.length > 0 && (
        <div className={notice}>
          An accepted edit was interrupted and needs attention.{" "}
          <button className={linkButton} onClick={() => setTab("edits")}>
            Open Edits
          </button>
        </div>
      )}
      {proposed.length > 0 && (
        <div className={notice}>
          {proposed.length === 1
            ? "One note suggested by the assistant is waiting for your decision."
            : `${proposed.length} notes suggested by the assistant are waiting for your decision.`}
        </div>
      )}

      <details className="workspace-overview-details">
        <summary>
          Project setup<span>Paper, reference results & folder</span>
        </summary>
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
                The run that new results are compared against. Runs appear here
                after an experiment completes in Research tools.
              </p>
              {data.ledger.staleClaims.length > 0 && (
                <p className="text-xs text-amber-700 dark:text-amber-300">
                  {data.ledger.staleClaims.length === 1
                    ? "One claim rests on evidence that is out of date."
                    : `${data.ledger.staleClaims.length} claims rest on evidence that is out of date.`}
                </p>
              )}
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
                  This folder could not be found. It may have been moved or is
                  on a disk that is not connected.
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
                      On this platform the assistant can edit a working copy,
                      but its edits cannot be copied back into this folder.
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
      </details>

      {summaryNotes.length > 0 && (
        <Card title="Project summary">
          {summaryNotes.length ? (
            summaryNotes.map((n) => (
              <p key={n.id} className="whitespace-pre-wrap text-sm">
                <span className={`font-semibold ${muted}`}>
                  {noteKindLabel(n.kind)}
                </span>
                <br />
                {n.body}
              </p>
            ))
          ) : (
            <p className={muted}>
              Nothing here yet. Mark a note “Show in project summary” to keep
              the research question, key assumptions, and next step in view.
            </p>
          )}

          <details>
            <summary className={`cursor-pointer ${muted}`}>
              What the assistant is told
            </summary>
            <pre className="mt-3 max-h-64 overflow-auto whitespace-pre-wrap text-xs">
              {data.contextPreview}
            </pre>
          </details>
        </Card>
      )}

      {openTasks.length > 0 && (
        <section className="workspace-overview-next">
          <div>
            <h2>Next up</h2>
            <button className={linkButton} onClick={() => setTab("tasks")}>
              All action items
            </button>
          </div>
          {openTasks.slice(0, 3).map((task) => (
            <button key={task.id} type="button" onClick={() => setTab("tasks")}>
              {task.body.objective}
            </button>
          ))}
        </section>
      )}

      <details className="workspace-overview-details">
        <summary>
          Project notes<span>{data.notes.length} saved</span>
        </summary>
        <div className="workspace-overview-details-body">
          <section className="space-y-3">
            <h2 className="font-semibold">Notes</h2>
            <p className={muted}>
              Record decisions, assumptions, questions, and next steps so they
              survive across conversations.
            </p>
            <div className="flex flex-wrap gap-2 sm:flex-nowrap">
              <select
                aria-label="Note type"
                className="rounded-md border border-gray-300 bg-transparent px-2 text-xs dark:border-neutral-700"
                value={noteKind}
                onChange={(e) =>
                  setNoteKind(e.target.value as ResearchNote["kind"])
                }
              >
                {NOTE_KINDS.map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
              <textarea
                aria-label="New note"
                value={noteBody}
                onChange={(e) => setNoteBody(e.target.value)}
                rows={2}
                className={input}
                placeholder="Write a note…"
              />
              <button
                className={button}
                disabled={!noteBody.trim()}
                onClick={() =>
                  void run(async () => {
                    await workbenchClient.createNote({
                      workspaceId,
                      paperId: null,
                      kind: noteKind,
                      body: noteBody,
                      state: "accepted",
                      origin: "user",
                      pinned: false,
                      operationId: op(),
                    });
                    setNoteBody("");
                  })
                }
              >
                Save note
              </button>
            </div>
            {!data.notes.length && <p className={muted}>No notes yet.</p>}
            <div className="grid gap-3 lg:grid-cols-2">
              {data.notes.map((n) => (
                <NoteCard key={n.id} note={n} api={props} />
              ))}
            </div>
            {data.noteHistory?.length ? (
              <details
                className={`rounded-md border border-gray-200 p-3 dark:border-neutral-800 ${muted}`}
              >
                <summary className="cursor-pointer">
                  Earlier wording of edited notes
                </summary>
                {data.noteHistory.map((item, index) => (
                  <div key={`${item.noteId}-${index}`} className="mt-3">
                    <p>{formatDate(item.createdAt)}</p>
                    {item.details.previous ? (
                      <p className="whitespace-pre-wrap text-gray-700 dark:text-neutral-300">
                        {item.details.previous.body}
                      </p>
                    ) : (
                      <p>
                        Only the note’s settings changed; the earlier text was
                        not kept.
                      </p>
                    )}
                  </div>
                ))}
              </details>
            ) : null}
          </section>
        </div>
      </details>
    </div>
  );
}
