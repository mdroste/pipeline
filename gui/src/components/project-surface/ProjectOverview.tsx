// The project overview answers two questions from state the app already
// computed: what state is the paper in, and what went out of date since the
// researcher last looked. Sections with nothing to report do not render, and
// every row carries one action. Projects with no folder or paper have nothing
// to derive, so they get a short conversation-first page instead.

import { useEffect, useMemo, useRef, useState } from "react";
import type {
  TranscriptItem,
  WorkbenchSession,
} from "../../lib/workbenchTypes";
import { workbenchClient } from "../../lib/workbenchClient";
import { projectDate } from "../../lib/projectIndex";
import { router } from "../../lib/router";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import type { ContextItem } from "../../lib/deskClient";
import {
  BRIEF_PROMPT,
  briefNotes,
  currentManuscript,
  draftBlock,
  isThinProject,
  leftOff,
  projectSessions,
  recordVisit,
  repositoryBlock,
  roundBlock,
  sinceBlock,
  syncBlock,
  targetSummary,
  type OverviewAction,
  type OverviewBlock,
  type Visit,
} from "../../lib/projectOverview";
import useProjectSignals from "../../hooks/useProjectSignals";
import { payloadText } from "../WorkspaceConversationView";
import { isOpenTask, noteKindLabel, op, type SurfaceApi } from "./shared";
import ProjectNotes from "./ProjectNotes";
import "./ProjectHome.css";

interface Props extends SurfaceApi {
  sessions: WorkbenchSession[];
  /** False while another project view is in front; derived state is not reloaded then. */
  active?: boolean;
  /** The loaded conversation, used to recall the last request made in it. */
  transcript?: { sessionId: string; items: TranscriptItem[] } | null;
  onResume: (id: string) => void;
  onStartConversation: () => void;
  /** Opens a new conversation with this text as an unsent draft. */
  onAsk: (prompt: string, context?: ContextItem[]) => void;
  onAllProjects?: () => void;
  onOpenDocument: (revisionId: string) => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
  showProjectName?: boolean;
}

const KEEP_ALIVE_MS = 5 * 60_000;

function Block({
  block,
  meta,
  busy = false,
  onAction,
}: {
  block: OverviewBlock;
  meta?: string;
  /** True while derived state is loading; a remote check cannot be repeated meanwhile. */
  busy?: boolean;
  onAction: (action: OverviewAction) => void;
}) {
  return (
    <section className="project-overview-block" aria-label={block.title}>
      <h2>
        {block.title}
        {meta && <span className="project-home-meta"> · {meta}</span>}
      </h2>
      {block.rows.map((row) => (
        <div
          className="project-overview-row"
          key={row.id}
          data-attention={row.attention || undefined}
        >
          <div>
            <p>{row.text}</p>
            {row.detail && <p className="project-home-meta">{row.detail}</p>}
          </div>
          <button
            type="button"
            className="project-home-link"
            disabled={busy && row.action.kind === "fetch"}
            onClick={() => onAction(row.action)}
          >
            {busy && row.action.kind === "fetch"
              ? "Checking…"
              : `${row.action.label} →`}
          </button>
        </div>
      ))}
    </section>
  );
}

export default function ProjectOverview(props: Props) {
  const {
    data,
    workspace,
    workspaceId,
    sessions,
    active = true,
    transcript = null,
    onResume,
    onStartConversation,
    onAsk,
    onAllProjects,
    onOpenDocument,
    setTab,
    run,
  } = props;
  const [question, setQuestion] = useState("");
  const [editingQuestion, setEditingQuestion] = useState(false);
  const [saving, setSaving] = useState(false);
  const notes = useRef<HTMLDetailsElement>(null);

  const thin = isThinProject(data, workspace);
  const own = useMemo(
    () => projectSessions(sessions, workspaceId),
    [sessions, workspaceId],
  );
  const sessionIds = useMemo(() => own.map((session) => session.id), [own]);
  const { signals, loading, reload, checkRemote } = useProjectSignals({
    workspaceId,
    root: workspace?.root ?? null,
    sessionIds,
    enabled: active && !thin,
    stamp: data,
  });

  const files = data.inventory?.body.files;
  const [visit, setVisit] = useState<Visit>(() =>
    recordVisit(workspaceId, new Date(), files),
  );
  useEffect(() => {
    setVisit(recordVisit(workspaceId, new Date(), files));
  }, [workspaceId, files]);
  useEffect(() => {
    const timer = setInterval(() => {
      if (document.visibilityState === "visible")
        recordVisit(workspaceId, new Date(), null);
    }, KEEP_ALIVE_MS);
    return () => clearInterval(timer);
  }, [workspaceId]);

  const brief = briefNotes(data);
  const resume = leftOff(data, sessions, workspaceId, transcript, payloadText);
  const manuscript = currentManuscript(data).paper;
  const target = targetSummary(data.settings.body, new Date());
  const blocks = thin
    ? []
    : [
        draftBlock(data, signals),
        syncBlock(data, signals),
        roundBlock(signals),
        repositoryBlock(signals, new Date()),
      ].filter((block): block is OverviewBlock => Boolean(block));
  const since = thin ? null : sinceBlock(data, signals, visit);

  const openTasks = data.tasks
    .filter((task) => isOpenTask(task.body.status))
    .sort(
      (a, b) =>
        Number(a.body.status === "deferred") -
          Number(b.body.status === "deferred") ||
        b.updatedAt.localeCompare(a.updatedAt),
    );
  const proposed = data.notes.filter((note) => note.state === "proposed");
  const others = own.filter((session) => session.id !== resume?.session.id);

  function showNotes() {
    if (notes.current) {
      notes.current.open = true;
      notes.current.scrollIntoView({ block: "start", behavior: "smooth" });
    }
  }
  const act = (action: OverviewAction) => {
    if (action.kind === "tab") setTab(action.tab);
    else if (action.kind === "ask") onAsk(action.prompt);
    else if (action.kind === "document") onOpenDocument(action.revisionId);
    else if (action.kind === "run")
      void router.navigate({ page: "history", runId: action.runId });
    else if (action.kind === "review") void router.navigate({ page: "main" });
    else if (action.kind === "automation")
      void router.navigate({ page: "tasks", taskId: action.taskId });
    else if (action.kind === "link")
      void Promise.resolve()
        .then(() => openExternal(action.url))
        .catch(() => props.reportError("The link could not be opened."));
    else
      void checkRemote().catch((cause: Error) =>
        props.reportError(cause.message),
      );
  };
  const saveQuestion = async () => {
    if (!question.trim() || saving) return;
    setSaving(true);
    await run(async () => {
      await workbenchClient.createNote({
        workspaceId,
        paperId: null,
        kind: "question",
        body: question.trim(),
        state: "accepted",
        origin: "user",
        pinned: true,
        operationId: op(),
      });
      setQuestion("");
      setEditingQuestion(false);
    });
    setSaving(false);
  };
  const draftBrief = () =>
    onAsk(
      BRIEF_PROMPT,
      manuscript?.revision
        ? [
            {
              role: "main",
              object: {
                kind: "paper",
                id: manuscript.revision.id,
                revision: manuscript.revision.contentHash,
              },
            },
          ]
        : undefined,
    );

  return (
    <div className="project-home">
      <div className="project-overview" data-thin={thin || undefined}>
        {onAllProjects && (
          <button
            type="button"
            className="project-home-link muted project-overview-back"
            onClick={onAllProjects}
          >
            ← All projects
          </button>
        )}
        <header className="project-home-heading">
          <div>
            <h1>
              {props.showProjectName
                ? (workspace?.name ?? "Project overview")
                : "Project overview"}
            </h1>
            {target && (
              <p>
                {target.label} · {target.date} ·{" "}
                {target.days > 1
                  ? `in ${target.days} days`
                  : target.days === 1
                    ? "tomorrow"
                    : target.days === 0
                      ? "today"
                      : target.days === -1
                        ? "yesterday"
                        : `${-target.days} days ago`}
              </p>
            )}
          </div>
          <button
            type="button"
            className="project-home-link muted"
            onClick={() => setTab("project-settings")}
          >
            Project settings
          </button>
        </header>

        <section className="project-brief" aria-label="Research brief">
          <h2>Research brief</h2>
          {brief.map((note) => (
            <div key={note.id}>
              <p>{note.body}</p>
              <p className="project-home-meta">
                {noteKindLabel(note.kind)} · {projectDate(note.updatedAt)}
              </p>
            </div>
          ))}
          {!brief.length && !editingQuestion && (
            <div className="project-overview-tools">
              {manuscript && (
                <button
                  type="button"
                  className="project-home-link"
                  onClick={draftBrief}
                >
                  Draft a brief from the paper
                </button>
              )}
              <button
                type="button"
                className="project-home-link"
                onClick={() => setEditingQuestion(true)}
              >
                Add research question
              </button>
            </div>
          )}
          {editingQuestion && (
            <form
              onSubmit={(event) => {
                event.preventDefault();
                void saveQuestion();
              }}
              className="space-y-3 mt-3"
            >
              <label className="block text-sm">
                Research question
                <textarea
                  className="mt-2 w-full rounded-md border border-gray-300 bg-transparent px-3 py-2 dark:border-neutral-700"
                  value={question}
                  onChange={(event) => setQuestion(event.target.value)}
                  rows={3}
                  maxLength={10000}
                  autoFocus
                />
              </label>
              <div className="flex gap-3">
                <button
                  className="project-home-button primary"
                  disabled={saving || !question.trim()}
                >
                  {saving ? "Saving…" : "Save question"}
                </button>
                <button
                  type="button"
                  className="project-home-button"
                  disabled={saving}
                  onClick={() => setEditingQuestion(false)}
                >
                  Cancel
                </button>
              </div>
            </form>
          )}
          {!!brief.length && (
            <button
              type="button"
              className="project-home-link"
              onClick={showNotes}
            >
              Manage brief notes
            </button>
          )}
        </section>

        <section className="project-resume" aria-label="Where you left off">
          <div>
            <p className="project-home-meta">
              {resume
                ? `Where you left off · ${projectDate(resume.session.updatedAt)}`
                : "Start with a conversation"}
            </p>
            <h2>
              {resume?.session.title ?? "What would you like to work on?"}
            </h2>
            {!resume && (
              <p className="project-home-meta">
                Develop an idea, work through a result, or begin drafting. You
                can add a paper or folder at any time.
              </p>
            )}
            {resume?.lastAsk && (
              <p className="project-resume-line">
                <span>You last asked</span> {resume.lastAsk}
              </p>
            )}
            {resume?.draft && (
              <p className="project-resume-line">
                <span>Unsent draft</span> {resume.draft}
              </p>
            )}
            {!!resume?.runs.total && (
              <p className="project-resume-line">
                <span>Runs started there</span> {resume.runs.total}
                {resume.runs.failed
                  ? `, ${resume.runs.failed} unsuccessful`
                  : ""}
              </p>
            )}
            {resume?.nextSteps.map((note) => (
              <p className="project-resume-line" key={note.id}>
                <span>{noteKindLabel(note.kind)}</span> {note.body}
              </p>
            ))}
          </div>
          <button
            type="button"
            className="project-home-button primary"
            onClick={() =>
              resume ? onResume(resume.session.id) : onStartConversation()
            }
          >
            {resume ? "Resume conversation" : "Start conversation"} →
          </button>
        </section>

        {!thin && loading && !blocks.length && !since && (
          <p role="status" className="project-home-meta">
            Checking the state of the draft…
          </p>
        )}
        <div className="project-overview-columns">
          {/* Left: the state of the paper. Right: what happened and what waits. */}
          {!!blocks.length && (
            <div>
              {blocks.map((block) => (
                <Block
                  key={block.id}
                  block={block}
                  busy={loading}
                  onAction={act}
                />
              ))}
            </div>
          )}
          {(since ||
            !!proposed.length ||
            !!openTasks.length ||
            !!others.length) && (
            <div>
              {since && (
                <Block
                  block={since}
                  meta={visit.since ? projectDate(visit.since) : undefined}
                  onAction={act}
                />
              )}
              {(!!proposed.length || !!openTasks.length) && (
                <section
                  className="project-overview-block"
                  aria-label="Needs your decision"
                >
                  <h2>Needs your decision</h2>
                  {!!proposed.length && (
                    <div className="project-overview-row" data-attention>
                      <div>
                        <p>
                          {proposed.length} suggested{" "}
                          {proposed.length === 1
                            ? "note awaits"
                            : "notes await"}{" "}
                          your decision
                        </p>
                      </div>
                      <button
                        type="button"
                        className="project-home-link"
                        onClick={showNotes}
                      >
                        Review suggested notes →
                      </button>
                    </div>
                  )}
                  {openTasks.slice(0, 3).map((task) => (
                    <div className="project-overview-row" key={task.id}>
                      <div>
                        <p>{task.body.objective}</p>
                        <p className="project-home-meta">
                          Action item ·{" "}
                          {task.body.status === "deferred"
                            ? "Later"
                            : task.body.status === "investigating"
                              ? "In progress"
                              : "Open"}
                        </p>
                      </div>
                      <button
                        type="button"
                        className="project-home-link"
                        onClick={() => setTab("tasks")}
                      >
                        Open →
                      </button>
                    </div>
                  ))}
                  {openTasks.length > 3 && (
                    <button
                      type="button"
                      className="project-home-link mt-3"
                      onClick={() => setTab("tasks")}
                    >
                      All {openTasks.length} action items
                    </button>
                  )}
                </section>
              )}
              {!!others.length && (
                <section
                  className="project-overview-block"
                  aria-label="Other conversations"
                >
                  <h2>Other conversations</h2>
                  {others.slice(0, 4).map((session) => (
                    <div className="project-overview-row" key={session.id}>
                      <div>
                        <p className="line-clamp-2">{session.title}</p>
                        <p className="project-home-meta">
                          {projectDate(session.updatedAt)}
                        </p>
                      </div>
                      <button
                        type="button"
                        className="project-home-link"
                        onClick={() => onResume(session.id)}
                      >
                        Resume →
                      </button>
                    </div>
                  ))}
                </section>
              )}
            </div>
          )}
        </div>

        {thin && (
          <p className="project-home-meta project-overview-thin">
            Import a paper or attach the project folder and this page will track
            the state of the draft: open review findings, numbers and claims
            that no longer match their results, and what changed since your last
            visit.
          </p>
        )}
        <div className="project-overview-tools">
          <button
            type="button"
            className="project-home-link"
            onClick={() => props.onImportPaper(false)}
          >
            Import paper
          </button>
          <button
            type="button"
            className="project-home-link"
            onClick={() =>
              workspace?.root ? setTab("files") : props.onAttachFolder()
            }
          >
            {workspace?.root ? "Open files" : "Attach folder"}
          </button>
          {resume && (
            <button
              type="button"
              className="project-home-link"
              onClick={onStartConversation}
            >
              New conversation
            </button>
          )}
          <button
            type="button"
            className="project-home-link"
            onClick={() => setTab("tasks")}
          >
            {openTasks.length ? "Action items" : "Add an action item"}
          </button>
          {!thin && (
            <button
              type="button"
              className="project-home-link"
              disabled={loading}
              onClick={reload}
            >
              {loading ? "Checking…" : "Check again"}
            </button>
          )}
        </div>
        <details
          ref={notes}
          className="workspace-overview-details project-overview-notes"
        >
          <summary>
            Project notes<span>{data.notes.length} saved</span>
          </summary>
          <div className="workspace-overview-details-body">
            <ProjectNotes {...props} />
          </div>
        </details>
      </div>
    </div>
  );
}
