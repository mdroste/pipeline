import { useProjectEnvironment } from "../../lib/projectEnvironment";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import type {
  TranscriptItem,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";
import type { ContextItem } from "../../lib/deskClient";
import { projectDate } from "../../lib/projectIndex";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { attentionRows } from "../../lib/project-overview/arrange";
import {
  loadAttentionPreferences,
  saveAttentionPreferences,
  deferred,
  type AttentionPreferences,
} from "../../lib/project-overview/attention";
import {
  briefNotes,
  leftOff,
  projectSessions,
} from "../../lib/project-overview/conversations";
import {
  draftBlock,
  isThinProject,
} from "../../lib/project-overview/manuscript";
import {
  markActivitySeen,
  type Visit,
} from "../../lib/project-overview/visits";
import {
  recentWork,
  type RecentWork,
} from "../../lib/project-overview/continuation";
import { repositoryBlock } from "../../lib/project-overview/repository";
import { roundBlock, syncBlock } from "../../lib/project-overview/review";
import { sinceBlock } from "../../lib/project-overview/since";
import { targetSummary } from "../../lib/project-overview/target";
import type {
  OverviewAction,
  OverviewBlock,
} from "../../lib/project-overview/types";
import useProjectSignals from "../../hooks/useProjectSignals";
import { payloadText } from "../../lib/workspaceTranscript";
import type { SurfaceApi } from "./shared";
import WorkspaceIcon from "../WorkspaceIcon";
import OverviewDialog, { OverviewPane } from "./OverviewDialog";
import OverviewRows from "./OverviewRows";
import OverviewBrief from "./OverviewBrief";
import OverviewWorkingSet from "./OverviewWorkingSet";
import OverviewAttention from "./OverviewAttention";
import OverviewSources from "./OverviewSources";
import "../../ui/page.css";
import "./ProjectHome.css";
import "./ProjectOverview.css";

interface Props extends SurfaceApi {
  sessions: WorkbenchSession[];
  active?: boolean;
  transcript?: { sessionId: string; items: TranscriptItem[] } | null;
  onResume: (id: string) => void;
  onStartConversation: () => void;
  onAsk: (prompt: string, context?: ContextItem[]) => void;
  onAllProjects?: () => void;
  onOpenDocument: (
    revisionId: string,
    mode?: "read" | "edit" | "compare",
  ) => void;
  onOpenWork?: (work: RecentWork) => void;
  onOpenTarget?: (action: OverviewAction) => void;
  onImportPaper: (folder: boolean) => void;
  onAttachFolder: () => void;
  showProjectName?: boolean;
  options?: (onClose: () => void) => ReactNode;
  error?: string | null;
  onDismissError?: () => void;
  busy?: boolean;
  initialPanel?: "brief" | "notes" | "draft";
  onPanelChange?: (panel: string | null) => void;
}

export default function ProjectOverview(props: Props) {
  const { navigate, openExternal, recordVisit, embeddedDialogs } =
    useProjectEnvironment();
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
    onOpenDocument,
    setTab,
  } = props;
  const [panel, setLocalPanel] = useState<string | null>(
    props.initialPanel ?? null,
  );
  const [noteId, setNoteId] = useState<string>();
  const setPanel = (next: string | null) => {
    setLocalPanel(next);
    props.onPanelChange?.(next);
  };
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
  const [preferences, setPreferences] = useState<AttentionPreferences>(() =>
    embeddedDialogs ? {} : loadAttentionPreferences(workspaceId),
  );
  const [work, setWork] = useState<RecentWork[]>(() =>
    embeddedDialogs ? [] : recentWork(workspaceId),
  );
  useEffect(() => {
    if (!active) {
      setPanel(null);
      return;
    }
    setVisit(recordVisit(workspaceId, new Date(), files));
    setWork(embeddedDialogs ? [] : recentWork(workspaceId));
  }, [active, workspaceId, files, embeddedDialogs]);
  const brief = briefNotes(data).find(
    (note) => note.pinned || data.settings.body.briefNoteIds.includes(note.id),
  );
  const conversation = leftOff(
    data,
    sessions,
    workspaceId,
    transcript,
    payloadText,
  );
  const continuation =
    props.onOpenWork &&
    (!conversation ||
      (!conversation.draft &&
        Date.parse(work[0]?.at ?? "") >=
          Date.parse(conversation.session.updatedAt)))
      ? work[0]
      : undefined;
  const target = targetSummary(data.settings.body, new Date());
  const derived = thin
    ? []
    : [
        draftBlock(data, signals),
        syncBlock(data),
        roundBlock(signals),
        repositoryBlock(signals, new Date()),
      ].filter((block): block is OverviewBlock => Boolean(block));
  const attention = attentionRows(data, derived);
  const due = attention.filter((row) => !deferred(row, preferences));
  const since = thin ? null : sinceBlock(data, signals, visit);
  const detailBlock = [...derived, since].find((block) => block?.id === panel);
  const repository = signals.repository;
  const close = () => setPanel(null);
  const navigateTab = (tab: Parameters<typeof setTab>[0]) => {
    close();
    setTab(tab);
  };
  const act = (action: OverviewAction) => {
    if (action.kind !== "fetch" && action.kind !== "link") close();
    if (action.kind === "tab") {
      if (action.targetId && props.onOpenTarget) props.onOpenTarget(action);
      else {
        if (action.tab === "tasks" && action.targetId)
          props.setTaskId(action.targetId);
        setTab(action.tab);
      }
    } else if (action.kind === "file") {
      if (props.onOpenTarget) props.onOpenTarget(action);
      else setTab("files");
    } else if (action.kind === "ask") {
      const context = action.context ?? [];
      onAsk(
        action.prompt,
        action.target && !context.some((item) => item.role === "main")
          ? [{ role: "main", object: action.target }, ...context]
          : context,
      );
    } else if (action.kind === "document")
      onOpenDocument(action.revisionId, action.mode);
    else if (action.kind === "run")
      void navigate({ page: "history", runId: action.runId });
    else if (action.kind === "review") setTab("review");
    else if (action.kind === "automation")
      void navigate({ page: "tasks", taskId: action.taskId });
    else if (action.kind === "notes") {
      setNoteId(action.noteId);
      setPanel("notes");
    } else if (action.kind === "link")
      void Promise.resolve()
        .then(() => openExternal(action.url))
        .catch(() => props.reportError("The link could not be opened."));
    else
      void checkRemote().catch((cause: unknown) =>
        props.reportError(workbenchErrorMessage(cause)),
      );
  };
  const savePreferences = (next: AttentionPreferences) => {
    try {
      if (!embeddedDialogs) saveAttentionPreferences(workspaceId, next);
      setPreferences(next);
    } catch {
      props.reportError("The priority could not be saved. Please try again.");
    }
  };
  const acknowledge = () => {
    try {
      setVisit(
        embeddedDialogs
          ? { since: new Date().toISOString(), baseline: null }
          : markActivitySeen(workspaceId, new Date(), files),
      );
      close();
    } catch {
      props.reportError("The activity marker could not be saved.");
    }
  };
  const hasDocuments = data.papers.some(
    (item) => item.revision && !item.paper.archived,
  );
  return (
    <div
      className="project-home command-center research-dashboard"
      data-thin={thin || undefined}
    >
      <header className="overview-heading">
        <div className="overview-identity">
          {props.onAllProjects && (
            <div className="overview-eyebrow">
              <button type="button" onClick={props.onAllProjects}>
                ← All projects
              </button>
            </div>
          )}
          {props.showProjectName !== false && (
            <h1 title={workspace?.name}>{workspace?.name ?? "Project"}</h1>
          )}
          {target && (
            <p className="overview-deadline">{`${target.label} · ${target.date} · ${target.days > 1 ? `in ${target.days} days` : target.days === 1 ? "tomorrow" : target.days === 0 ? "today" : target.days === -1 ? "yesterday" : `${-target.days} days ago`}`}</p>
          )}
        </div>
        <div className="overview-actions">
          <button
            type="button"
            className="ui-button"
            onClick={onStartConversation}
          >
            Ask
          </button>
          <button
            type="button"
            className="ui-button"
            data-project-tour="brief"
            onClick={() => {
              setNoteId(undefined);
              setPanel("brief");
            }}
          >
            Context
          </button>
          <button
            type="button"
            className="ui-button"
            onClick={() => setPanel("options")}
            aria-haspopup="dialog"
          >
            <WorkspaceIcon name="settings" />
            Options
          </button>
        </div>
      </header>
      {brief && (
        <button
          type="button"
          className="overview-question-preview"
          title={brief.body}
          onClick={() => {
            setNoteId(brief.id);
            setPanel("brief");
          }}
        >
          {brief.body}
        </button>
      )}
      <div className="overview-grid">
        <section
          className="overview-card overview-resume"
          aria-label="Resume work"
        >
          <div className="overview-card-heading">
            <h2>Resume</h2>
            {(work.length > 1 || own.length > 0) && (
              <button
                className="project-home-link"
                type="button"
                onClick={() => setPanel("conversation")}
              >
                Recent work →
              </button>
            )}
          </div>
          <h3 title={continuation?.title ?? conversation?.session.title}>
            {continuation?.title ??
              conversation?.session.title ??
              "What would you like to work on?"}
          </h3>
          {(continuation || conversation) && (
            <p className="overview-resume-excerpt">
              {continuation
                ? continuation.location
                  ? `${continuation.location.path}${continuation.location.line ? ` · line ${continuation.location.line}` : ""}`
                  : continuation.tab === "documents"
                    ? "Saved document · reading position retained"
                    : "Your working view"
                : (conversation?.draft ?? conversation?.lastAsk)}
            </p>
          )}
          <div className="overview-card-footer">
            <span className="overview-meta">
              {continuation
                ? projectDate(continuation.at)
                : conversation
                  ? `${conversation.draft ? "Unsent draft · " : ""}${projectDate(conversation.session.updatedAt)}`
                  : ""}
            </span>
            <button
              type="button"
              className="ui-button ui-button--primary"
              onClick={() =>
                continuation
                  ? props.onOpenWork?.(continuation)
                  : conversation
                    ? onResume(conversation.session.id)
                    : onStartConversation()
              }
            >
              {continuation
                ? "Continue work"
                : conversation
                  ? "Resume conversation"
                  : "Start conversation"}{" "}
              →
            </button>
          </div>
        </section>
        <section
          className="overview-card overview-paper"
          aria-label="Working set"
        >
          <div className="overview-card-heading">
            <h2>Working set</h2>
            <button
              type="button"
              className="ui-button"
              onClick={() => setPanel("add")}
            >
              <WorkspaceIcon name="plus" />
              Add
            </button>
          </div>
          <OverviewWorkingSet
            data={data}
            workspace={workspace}
            onAction={act}
          />
          <div className="overview-card-footer overview-actions">
            {hasDocuments && (
              <button
                type="button"
                className="project-home-link"
                onClick={() => navigateTab("documents")}
              >
                All documents →
              </button>
            )}
            <button
              type="button"
              className="project-home-link"
              onClick={() =>
                workspace?.root ? navigateTab("files") : props.onAttachFolder()
              }
            >
              {workspace?.root ? "Open files" : "Attach folder"} →
            </button>
            {derived.some((block) => block.id === "draft") && (
              <button
                type="button"
                className="project-home-link"
                data-project-tour="open-report"
                onClick={() => setPanel("draft")}
              >
                Builds & reviews →
              </button>
            )}
          </div>
        </section>
        {due.length > 0 && (
          <section
            className="overview-card overview-decisions"
            aria-label="Attention"
          >
            <div className="overview-card-heading">
              <h2>Attention</h2>
              <button
                type="button"
                className="project-home-link"
                onClick={() => setPanel("needs")}
              >
                All items →
              </button>
            </div>
            <OverviewAttention
              rows={attention}
              preferences={preferences}
              onPreferences={savePreferences}
              onAction={act}
              preview
            />
          </section>
        )}
        {since && (
          <section
            className="overview-card overview-activity"
            aria-label="Recent changes"
          >
            <div className="overview-card-heading">
              <h2>Recent changes</h2>
              <button
                type="button"
                className="project-home-link"
                onClick={() => setPanel("since")}
              >
                Inspect changes →
              </button>
            </div>
            <OverviewRows block={since} preview onAction={act} pageSize={2} />
          </section>
        )}
      </div>
      <footer className="overview-footer">
        {repository ? (
          <button
            type="button"
            className="project-home-link overview-repository"
            aria-label={`Repository: ${repository.branch ?? "Detached HEAD"}`}
            onClick={() => setPanel("repository")}
          >
            {repository.branch ?? "Repository"}
            <span>
              {" "}
              · {repository.changed + repository.untracked} local changes
              {signals.sources?.repository?.state === "error"
                ? " · last known"
                : ""}
              {repository.upstream
                ? repository.fetchedAt
                  ? ` · ${repository.ahead} ahead · ${repository.behind} behind at last fetch`
                  : " · remote unchecked"
                : ""}
            </span>
          </button>
        ) : (
          <span />
        )}
        <div className="overview-actions">
          {Object.values(signals.sources ?? {}).some(
            (source) =>
              source.state === "error" ||
              (source.state === "ready" && !source.complete),
          ) && (
            <button
              type="button"
              className="project-home-link"
              onClick={() => setPanel("draft")}
            >
              Some status unavailable →
            </button>
          )}
          {attention.length > 0 && !due.length && (
            <button
              type="button"
              className="project-home-link"
              onClick={() => setPanel("needs")}
            >
              Deferred items →
            </button>
          )}
          <button
            type="button"
            className="project-home-link"
            data-project-tour="status-tasks"
            onClick={() => navigateTab("tasks")}
          >
            Action items →
          </button>
        </div>
      </footer>
      <OverviewDialog
        onClose={close}
        error={props.error}
        onDismissError={props.onDismissError}
        busy={props.busy}
        open={active && panel !== null}
        title={
          panel === "draft"
            ? "Builds & reviews"
            : (detailBlock?.title ??
              {
                brief: "Project context",
                notes: "Project context",
                options: "Project options",
                add: "Add to project",
                conversation: "Recent work",
                needs: "Attention",
              }[panel ?? ""] ??
              "Attention")
        }
      >
        <OverviewPane open={active && (panel === "brief" || panel === "notes")}>
          <OverviewBrief
            {...props}
            selectedNoteId={noteId}
            onAsk={(prompt, context) => {
              close();
              onAsk(prompt, context);
            }}
          />
        </OverviewPane>
        <OverviewPane open={active && panel === "options"}>
          {props.options?.(close) ?? (
            <button
              type="button"
              className="ui-button"
              onClick={() => navigateTab("project-settings")}
            >
              Project settings
            </button>
          )}
        </OverviewPane>
        <OverviewPane open={active && panel === "needs"}>
          {data.tasksCursor && (
            <p className="overview-meta">
              Showing action items from the first page. Open All action items to
              load more.
            </p>
          )}
          <OverviewAttention
            rows={attention}
            preferences={preferences}
            onPreferences={savePreferences}
            onAction={act}
          />
          <button
            type="button"
            className="ui-button"
            onClick={() => navigateTab("tasks")}
          >
            All action items
          </button>
        </OverviewPane>
        <OverviewPane open={active && panel === "add"}>
          <div className="overview-tool-grid">
            <button
              type="button"
              className="ui-button"
              onClick={() => {
                close();
                props.onImportPaper(false);
              }}
            >
              Add document
            </button>
            <button
              type="button"
              className="ui-button"
              onClick={() => {
                close();
                props.onImportPaper(true);
              }}
            >
              Import LaTeX folder
            </button>
            <button
              type="button"
              className="ui-button"
              onClick={() => {
                close();
                workspace?.root ? setTab("files") : props.onAttachFolder();
              }}
            >
              {workspace?.root ? "Open files" : "Attach folder"}
            </button>
            <button
              type="button"
              className="ui-button"
              onClick={() => setPanel("notes")}
            >
              Add a note
            </button>
            <button
              type="button"
              className="ui-button"
              onClick={() => navigateTab("tasks")}
            >
              Add an action item
            </button>
          </div>
        </OverviewPane>
        <OverviewPane open={active && Boolean(detailBlock)}>
          {detailBlock && (
            <OverviewRows
              key={detailBlock.id}
              block={detailBlock}
              busy={loading}
              onAction={act}
              pageSize={6}
            />
          )}
          {panel === "since" && (
            <button
              type="button"
              className="ui-button"
              disabled={loading}
              onClick={acknowledge}
            >
              Mark these changes seen
            </button>
          )}
          {panel === "repository" && (
            <button
              type="button"
              className="ui-button"
              onClick={() => navigateTab("version-control")}
            >
              Version control
            </button>
          )}
        </OverviewPane>
        <OverviewPane open={active && panel === "draft"}>
          <OverviewSources
            signals={signals}
            onRefresh={reload}
            tasksPartial={Boolean(data.tasksCursor)}
          />
        </OverviewPane>
        <OverviewPane open={active && panel === "conversation"}>
          {work.map((item) => (
            <div className="overview-conversation" key={item.id}>
              <span>
                {item.title}
                <small>{projectDate(item.at)}</small>
              </span>
              <button
                type="button"
                className="project-home-link"
                onClick={() => {
                  close();
                  props.onOpenWork?.(item);
                }}
              >
                Continue →
              </button>
            </div>
          ))}
          {own.map((session) => (
            <div className="overview-conversation" key={session.id}>
              <span>
                {session.title}
                <small>{projectDate(session.updatedAt)}</small>
              </span>
              <button
                type="button"
                className="project-home-link"
                onClick={() => {
                  close();
                  onResume(session.id);
                }}
              >
                Resume conversation →
              </button>
            </div>
          ))}
        </OverviewPane>
      </OverviewDialog>
    </div>
  );
}
