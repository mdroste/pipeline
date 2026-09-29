import { useRef, useState } from "react";
import type { WorkbenchSession } from "../../lib/workbenchTypes";
import { workbenchClient } from "../../lib/workbenchClient";
import { projectDate } from "../../lib/projectIndex";
import { isOpenTask, noteKindLabel, op, type SurfaceApi } from "./shared";
import ProjectNotes from "./ProjectNotes";
import "./ProjectHome.css";

interface Props extends SurfaceApi {
  sessions: WorkbenchSession[];
  onResume: (id: string) => void;
  onStartConversation: () => void;
  onAllProjects?: () => void;
  onOpenDocument: (revisionId: string) => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
  showProjectName?: boolean;
}

export default function ProjectOverview(props: Props) {
  const {
    data,
    workspace,
    workspaceId,
    sessions,
    onResume,
    onStartConversation,
    onAllProjects,
    onOpenDocument,
    setTab,
    run,
  } = props;
  const [question, setQuestion] = useState("");
  const [editingQuestion, setEditingQuestion] = useState(false);
  const [saving, setSaving] = useState(false);
  const notes = useRef<HTMLDetailsElement>(null);
  const latest = sessions
    .filter((s) => s.workspaceId === workspaceId && !s.archivedAt)
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))[0];
  const accepted = data.notes.filter((note) => note.state === "accepted");
  const curated = accepted.filter((note) =>
    data.settings.body.briefNoteIds.includes(note.id),
  );
  const brief = curated.length
    ? curated
    : accepted
        .filter((note) => note.pinned || note.kind === "question")
        .sort(
          (a, b) =>
            Number(b.kind === "question") - Number(a.kind === "question") ||
            b.updatedAt.localeCompare(a.updatedAt),
        )
        .slice(0, 3);
  const openTasks = data.tasks
    .filter((task) => isOpenTask(task.body.status))
    .sort(
      (a, b) =>
        Number(a.body.status === "deferred") -
          Number(b.body.status === "deferred") ||
        b.updatedAt.localeCompare(a.updatedAt),
    );
  const proposed = data.notes.filter((note) => note.state === "proposed");
  const recovery = data.applications.filter((item) =>
    ["applying", "recovery_required"].includes(item.body.state),
  );
  const papers = data.papers.filter((paper) => paper.revision);
  const currentPaper = papers.find(
    (paper) => paper.revision?.id === data.settings.body.manuscriptRevisionId,
  );
  const recent = [
    ...sessions
      .filter((s) => s.workspaceId === workspaceId && !s.archivedAt)
      .map((s) => ({
        id: s.id,
        title: s.title,
        at: s.updatedAt,
        kind: "Conversation",
        open: () => onResume(s.id),
      })),
    ...papers.map((p) => ({
      id: p.revision!.id,
      title: p.paper.title,
      at: p.revision!.capturedAt,
      kind: p === currentPaper ? "Current paper" : "Document",
      open: () => onOpenDocument(p.revision!.id),
    })),
    ...accepted.map((n) => ({
      id: n.id,
      title: n.body,
      at: n.updatedAt,
      kind: `Saved ${noteKindLabel(n.kind).toLocaleLowerCase()}`,
      open: () => showNotes(),
    })),
  ]
    .sort((a, b) => b.at.localeCompare(a.at))
    .slice(0, 4);
  function showNotes() {
    if (notes.current) {
      notes.current.open = true;
      notes.current.scrollIntoView({ block: "start", behavior: "smooth" });
    }
  }
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
  return (
    <div className="project-home">
      <div className="project-overview">
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
          </div>
          <button
            type="button"
            className="project-home-link muted"
            onClick={() => setTab("project-settings")}
          >
            Project settings
          </button>
        </header>
        <section className="project-resume" aria-label="Continue research">
          <div>
            <p className="project-home-meta">
              {latest
                ? `Last conversation · ${projectDate(latest.updatedAt)}`
                : "Start with a conversation"}
            </p>
            <h2>{latest?.title ?? "What would you like to work on?"}</h2>
            {!latest && (
              <p className="project-home-meta">
                Develop an idea, work through a result, or begin drafting. You
                can add a paper or folder at any time.
              </p>
            )}
          </div>
          <button
            type="button"
            className="project-home-button primary"
            onClick={() =>
              latest ? onResume(latest.id) : onStartConversation()
            }
          >
            {latest ? "Resume conversation" : "Start conversation"} →
          </button>
        </section>
        <section className="project-brief" aria-label="Research brief">
          <h2>Research brief</h2>
          {brief.map((note) => (
            <div key={note.id}>
              <p>{note.body}</p>
              <p className="project-home-meta">
                {noteKindLabel(note.kind)} · Accepted note ·{" "}
                {projectDate(note.updatedAt)}
              </p>
            </div>
          ))}
          {!brief.length && !editingQuestion && (
            <>
              <p className="muted">
                Keep the research question and key decisions here for your next
                visit.
              </p>
              <button
                type="button"
                className="project-home-link"
                onClick={() => setEditingQuestion(true)}
              >
                Add research question
              </button>
            </>
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
        <div className="project-overview-columns">
          <section aria-label="Needs your attention">
            <h2>Needs your attention</h2>
            {!!recovery.length && (
              <div className="project-overview-item">
                <p>An accepted edit was interrupted.</p>
                <button
                  className="project-home-link project-home-attention"
                  onClick={() => setTab("edits")}
                >
                  Inspect edit recovery →
                </button>
              </div>
            )}
            {!!proposed.length && (
              <div className="project-overview-item">
                <p>
                  {proposed.length} suggested{" "}
                  {proposed.length === 1 ? "note awaits" : "notes await"} your
                  decision.
                </p>
                <button
                  className="project-home-link project-home-attention"
                  onClick={showNotes}
                >
                  Review suggested notes →
                </button>
              </div>
            )}
            {!!data.ledger.staleClaims.length && (
              <div className="project-overview-item">
                <p>
                  {data.ledger.staleClaims.length}{" "}
                  {data.ledger.staleClaims.length === 1
                    ? "claim uses"
                    : "claims use"}{" "}
                  evidence that is out of date.
                </p>
                <button
                  className="project-home-link"
                  onClick={() => setTab("evidence")}
                >
                  Inspect evidence →
                </button>
              </div>
            )}
            {openTasks.slice(0, 3).map((task) => (
              <div className="project-overview-item" key={task.id}>
                <button onClick={() => setTab("tasks")}>
                  {task.body.objective}
                </button>
                <p className="project-home-meta">
                  Action item ·{" "}
                  {task.body.status === "deferred"
                    ? "Later"
                    : task.body.status === "investigating"
                      ? "In progress"
                      : "Open"}
                </p>
              </div>
            ))}
            {!recovery.length &&
              !proposed.length &&
              !openTasks.length &&
              !data.ledger.staleClaims.length && (
                <p className="project-overview-item muted">
                  No pending decisions or action items.
                </p>
              )}
            <button
              type="button"
              className="project-home-link mt-4"
              onClick={() => setTab("tasks")}
            >
              {openTasks.length ? "All action items" : "Add an action item"}
            </button>
          </section>
          <section aria-label="Recent work">
            <h2>Recent work</h2>
            {recent.map((item) => (
              <div
                className="project-overview-item"
                key={`${item.kind}-${item.id}`}
              >
                <button className="line-clamp-2" onClick={item.open}>
                  {item.title}
                </button>
                <p className="project-home-meta">
                  {item.kind} · {projectDate(item.at)}
                </p>
              </div>
            ))}
            {!recent.length && (
              <p className="project-overview-item muted">
                Your conversations, documents, and accepted notes will appear
                here.
              </p>
            )}
          </section>
        </div>
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
          {latest && (
            <button
              type="button"
              className="project-home-link"
              onClick={onStartConversation}
            >
              New conversation
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
