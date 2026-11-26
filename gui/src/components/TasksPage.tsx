import useTabList from "../hooks/useTabList";
import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { taskClient } from "../lib/taskClient";
import { taskTime } from "../lib/taskPresentation";
import { type Schedule, type TaskRun } from "../lib/taskTypes";
import {
  automationClient,
  automationKindLabel,
  researchAutomationStatus,
  type AutomationRef,
  type AutomationDetail,
  type AutomationSummary,
} from "../lib/automationClient";
import { confirmDialog } from "./DialogService";
import NewAutomation from "./tasks/NewAutomation";
import Detail from "./tasks/Detail";
import ScheduleCard from "./tasks/ScheduleCard";
import { ResearchStatus, Status } from "./tasks/shared";
import PageHeader from "../ui/PageHeader";
import ProjectBreadcrumb from "./ProjectBreadcrumb";
import useRouteSnapshot from "../hooks/useRouteSnapshot";
import "./TasksPage.css";
import ReportIssueLink from "./ReportIssueLink";
const MissionDetail = lazy(() => import("./research-missions/Detail"));
const DiscoveryDetail = lazy(() => import("./self-discovery/Detail"));
import "./research-missions/Missions.css";
import "./self-discovery/discovery.css";

export default function TasksPage({
  entryRequest = 0,
  initialSessionId,
  onDirtyChange,
  onConversation,
  initialTaskId,
  initialDiscoveryId,
  initialAutomatePath,
}: {
  entryRequest?: number;
  initialSessionId?: string | null;
  onDirtyChange?: (dirty: boolean) => void;
  onConversation?: (id: string) => void | Promise<void>;
  initialTaskId?: string | null;
  initialDiscoveryId?: string | null;
  /** Open the builder prefilled to automate this input file. */
  initialAutomatePath?: string | null;
}) {
  const { route } = useRouteSnapshot();
  const [view, setView] = useState("active");
  const views = ["active", "scheduled", "history"];
  const tabList = useTabList(
    views,
    view,
    (id) => {
      void discardDraft().then((ok) => {
        if (!ok) return;
        setCreating(false);
        setView(id);
        setOffset(0);
      });
    },
    "manual",
  );
  const [creationRequest, setCreationRequest] = useState(0);
  const [useEntryContext, setUseEntryContext] = useState(true);
  const [runs, setRuns] = useState<AutomationSummary[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [selected, setSelected] = useState<AutomationDetail | null>(null);
  const selectedRef = useRef<AutomationRef | null>(
    initialDiscoveryId
      ? { kind: "discovery", id: initialDiscoveryId }
      : initialTaskId
        ? { kind: "task", id: initialTaskId }
        : null,
  );
  const [creating, setCreating] = useState(
    Boolean((initialSessionId || initialAutomatePath) && !initialTaskId),
  );
  const builderDirty = useRef(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const handleDirtyChange = useCallback(
    (dirty: boolean) => {
      builderDirty.current = dirty;
      onDirtyChange?.(dirty);
    },
    [onDirtyChange],
  );
  /** Ask before an in-page action throws away an unsaved builder draft. */
  const discardDraft = async () => {
    if (!creating || !builderDirty.current) return true;
    const discard = await confirmDialog(
      "This automation has not been previewed or saved. Discard it?",
      {
        title: "Discard automation draft?",
        confirmLabel: "Discard",
        cancelLabel: "Keep editing",
        destructive: true,
      },
    );
    if (discard) {
      setCreating(false);
      handleDirtyChange(false);
    }
    return discard;
  };
  const [issues, setIssues] = useState<
    { kind: string; id: string; diagnostic: string }[]
  >([]);
  const [error, setError] = useState("");
  const [background, setBackground] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const refreshVersion = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++refreshVersion.current;
    const selectedId = selectedRef.current;
    try {
      const [rows, planned, detail] = await Promise.all([
        view === "scheduled"
          ? Promise.resolve([])
          : automationClient.list(view, offset),
        view === "scheduled" ? taskClient.schedules() : Promise.resolve([]),
        // Task cleanup can remove a run a saved route still names. Research
        // records persist, so failed reads should preserve their open detail.
        selectedId
          ? automationClient.get(selectedId).catch((e) => {
              if (selectedId.kind === "task") return null;
              throw e;
            })
          : Promise.resolve(null),
      ]);
      // Listing schedules can isolate a damaged row. Read its diagnostic after
      // the list completes; a diagnostic read failure must not hide healthy work.
      const diagnostics = await taskClient.issues().then(
        (issues) => ({ issues, error: "" }),
        (error) => ({ issues: [], error: String(error) }),
      );
      if (version !== refreshVersion.current) return;
      setIssues(diagnostics.issues);
      setRuns(rows);
      setSchedules(planned);
      if (selectedId === selectedRef.current) {
        if (!detail) selectedRef.current = null;
        setSelected(detail);
      }
      setError(diagnostics.error);
    } catch (e) {
      if (
        version === refreshVersion.current &&
        selectedId === selectedRef.current
      )
        setError(String(e));
    } finally {
      if (version === refreshVersion.current) setLoading(false);
    }
  }, [view, offset]);
  useEffect(() => {
    refreshVersion.current++;
    selectedRef.current = initialDiscoveryId
      ? { kind: "discovery", id: initialDiscoveryId }
      : initialTaskId
        ? { kind: "task", id: initialTaskId }
        : null;
    setSelected(null);
    setCreating(
      Boolean((initialSessionId || initialAutomatePath) && !initialTaskId),
    );
    setView("active");
    setUseEntryContext(true);
    setOffset(0);
    void refresh();
    // Only new entry identities replace local selection; refresh is kept current
    // by the subscription effect below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    entryRequest,
    initialTaskId,
    initialSessionId,
    initialAutomatePath,
    initialDiscoveryId,
  ]);
  useEffect(() => {
    let disposed = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const queue = () => {
      if (!disposed && !timer)
        timer = setTimeout(() => {
          timer = null;
          if (!disposed) void refresh();
        }, 180);
    };
    void refresh();
    const listeners = [
      "tasks:changed",
      "missions:changed",
      "discovery:changed",
    ].map((event) => listen(event, queue));
    const errors = listen<string>("tasks:error", (e) => setError(e.payload));
    return () => {
      disposed = true;
      refreshVersion.current++;
      if (timer) clearTimeout(timer);
      for (const unlisten of listeners) void unlisten.then((f) => f());
      void errors.then((f) => f());
    };
  }, [refresh]);
  useEffect(() => {
    void taskClient
      .background()
      .then(setBackground)
      .catch((e) => setError(String(e)));
  }, []);
  const create = () => {
    setUseEntryContext(false);
    setCreationRequest((value) => value + 1);
    setCreating(true);
    selectedRef.current = null;
    setSelected(null);
  };
  const showDetail = (detail: AutomationDetail) => {
    selectedRef.current = { kind: detail.kind, id: detail.run.id };
    setSelected(detail);
    setCreating(false);
    void refresh();
  };
  const select = (run: TaskRun) => showDetail({ kind: "task", run });
  const updateDetail = (detail: AutomationDetail) => {
    const current = selectedRef.current;
    if (current?.kind === detail.kind && current.id === detail.run.id)
      showDetail(detail);
  };
  const closeDetail = () => {
    selectedRef.current = null;
    setSelected(null);
  };
  const openDetail = async (ref: AutomationRef) => {
    if (!(await discardDraft())) return;
    selectedRef.current = ref;
    setSelected(null);
    setCreating(false);
    try {
      const detail = await automationClient.get(ref);
      if (selectedRef.current === ref) setSelected(detail);
    } catch (e) {
      if (selectedRef.current === ref) setError(String(e));
    }
  };
  // The side pane renders at the top of its column (or below the list in the
  // narrow layout). Bring its heading into view when it opens from a scrolled
  // list; "nearest" leaves an already visible pane where it is.
  const paneKey = creating
    ? "new"
    : selected
      ? `${selected.kind}:${selected.run.id}`
      : null;
  useEffect(() => {
    if (!paneKey) return;
    const pane = contentRef.current?.querySelector(
      ":scope > .task-builder, :scope > .task-detail",
    );
    pane?.firstElementChild?.scrollIntoView?.({ block: "nearest" });
  }, [paneKey]);
  const detailProjectId = selected
    ? selected.kind === "task"
      ? selected.run.scope.workspaceId
      : selected.run.workspaceId
    : route.page === "tasks"
      ? route.projectId
      : undefined;
  return (
    <main className="ui-page ui-page--index tasks-page">
      {detailProjectId && (
        <ProjectBreadcrumb
          projectId={detailProjectId}
          location="Automation · All automations"
        />
      )}
      <PageHeader
        variant="standard"
        title="All automations"
        description="Schedule project work, review and revise papers, or investigate a research question."
        action={
          !creating && (
            <button className="ui-button ui-button--primary" onClick={create}>
              + New automation
            </button>
          )
        }
      />
      <div className="ui-page-toolbar">
        <div
          className="ui-page-tabs"
          role="tablist"
          aria-label="Automations view"
        >
          {views.map((v) => (
            <button {...tabList.tabProps(v)} key={v}>
              {v.charAt(0).toUpperCase() + v.slice(1)}
            </button>
          ))}
        </div>
        <label className="task-background">
          <input
            id="task-background-mode"
            type="checkbox"
            checked={background === true}
            onChange={(e) => {
              void taskClient
                .background(e.target.checked)
                .then(setBackground)
                .catch((e) => setError(String(e)));
            }}
          />
          Keep running when the window closes
        </label>
      </div>
      {issues?.length > 0 && (
        <details className="task-error">
          <summary>Automation records needing repair ({issues.length})</summary>
          <ul>
            {issues.map((issue) => (
              <li key={issue.kind + issue.id}>
                {issue.kind} {issue.id} · {issue.diagnostic}
              </li>
            ))}
          </ul>
        </details>
      )}
      {error && (
        <p className="task-error" role="alert">
          {error}
          <ReportIssueLink />
        </p>
      )}
      {views
        .filter((id) => id !== view)
        .map((id) => (
          <div key={id} {...tabList.panelProps(id)} hidden />
        ))}
      <div {...tabList.panelProps(view)}>
        {creating ? (
          <NewAutomation
            key={`${entryRequest}:${creationRequest}:${initialSessionId ?? ""}:${initialAutomatePath ?? ""}`}
            initialSessionId={initialSessionId}
            initialPath={useEntryContext ? initialAutomatePath : null}
            skipChooser={
              useEntryContext &&
              Boolean(initialSessionId || initialAutomatePath)
            }
            onDirtyChange={handleDirtyChange}
            onPrepared={(run) => {
              setView("active");
              select(run);
            }}
            onMissionPrepared={(mission) => {
              setView("active");
              showDetail({ kind: "research", run: mission });
            }}
            onDiscoveryStarted={(run) => {
              handleDirtyChange(false);
              setView("active");
              showDetail({ kind: "discovery", run });
            }}
            onClose={() =>
              void discardDraft().then((ok) => {
                if (ok) setCreating(false);
              })
            }
          />
        ) : (
          <div
            ref={contentRef}
            className={`tasks-content ${selected ? "tasks-content--split" : ""}`}
          >
            <section className="task-list">
              {loading ? (
                <p className="task-empty">Loading automations…</p>
              ) : view === "scheduled" ? (
                schedules.length ? (
                  schedules.map((s) => (
                    <ScheduleCard
                      key={`${s.id}-${s.revision}`}
                      schedule={s}
                      onUpdated={() => void refresh()}
                      onError={setError}
                      onOpenRun={async (id) => {
                        const run = await taskClient.get(id);
                        if (!(await discardDraft())) return;
                        setView("active");
                        select(run);
                      }}
                    />
                  ))
                ) : (
                  <div className="task-empty">
                    <h2>Make time for recurring work.</h2>
                    <p>Choose a schedule when creating an automation.</p>
                  </div>
                )
              ) : runs.length || offset > 0 ? (
                <>
                  {!runs.length && (
                    <p className="task-muted">No more automations.</p>
                  )}
                  {runs.map((run) => (
                    <button
                      aria-pressed={
                        selected?.kind === run.kind &&
                        selected.run.id === run.id
                      }
                      className={`task-list-row ${selected?.kind === run.kind && selected.run.id === run.id ? "is-selected" : ""}`}
                      key={`${run.kind}:${run.id}`}
                      onClick={() =>
                        void openDetail({ kind: run.kind, id: run.id })
                      }
                    >
                      <div>
                        <strong>{run.name}</strong>
                        <small className="task-muted">
                          {automationKindLabel(run.kind)}
                        </small>
                        <span>
                          {run.reason ||
                            (run.dueAt && run.dueAt > Date.now() / 1000
                              ? `Starts ${taskTime(run.dueAt)}`
                              : "Ready for the next step")}
                        </span>
                      </div>
                      {run.kind === "task" ? (
                        <Status state={run.state} />
                      ) : (
                        <ResearchStatus
                          state={run.state}
                          label={researchAutomationStatus(run.kind, run.state)}
                        />
                      )}
                    </button>
                  ))}
                  <div className="task-actions">
                    {offset > 0 && (
                      <button
                        onClick={() => setOffset(Math.max(0, offset - 50))}
                      >
                        Previous
                      </button>
                    )}
                    {runs.length >= 50 && (
                      <button onClick={() => setOffset(offset + 50)}>
                        Next
                      </button>
                    )}
                  </div>
                </>
              ) : (
                <div className="task-empty">
                  <div className="task-empty-symbol" aria-hidden="true">
                    ○—○—○
                  </div>
                  <h2>
                    {view === "history"
                      ? "Completed work will appear here."
                      : "No active automations."}
                  </h2>
                  <p>
                    Connect project conversations and reviews, pause for input,
                    or set a time to continue.
                  </p>
                  {view !== "history" && (
                    <button onClick={create}>Create an automation →</button>
                  )}
                </div>
              )}
            </section>
            {selected?.kind === "task" && (
              <Detail
                key={selected.run.id}
                run={selected.run}
                background={background}
                onBackgroundSetup={() => {
                  const control = document.getElementById(
                    "task-background-mode",
                  );
                  control?.scrollIntoView?.({ block: "center" });
                  control?.focus();
                }}
                onUpdated={(run) => updateDetail({ kind: "task", run })}
                onClose={closeDetail}
                onConversation={onConversation}
              />
            )}
            {selected?.kind === "research" && (
              <Suspense
                fallback={<p className="task-muted">Loading investigation…</p>}
              >
                <MissionDetail
                  key={selected.run.id}
                  mission={selected.run}
                  onUpdated={(run) => updateDetail({ kind: "research", run })}
                  onClose={closeDetail}
                  onConversation={onConversation}
                  onTask={async (id) => {
                    setView("active");
                    await openDetail({ kind: "task", id });
                  }}
                />
              </Suspense>
            )}
            {selected?.kind === "discovery" && (
              <Suspense
                fallback={
                  <p className="task-muted">Loading research discovery…</p>
                }
              >
                <DiscoveryDetail
                  key={selected.run.id}
                  run={selected.run}
                  onUpdated={(run) => updateDetail({ kind: "discovery", run })}
                  onClose={closeDetail}
                  onConversation={onConversation}
                />
              </Suspense>
            )}
          </div>
        )}
      </div>
    </main>
  );
}
