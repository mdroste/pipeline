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
import {
  type Schedule,
  type TaskRun,
  type TaskSummary,
} from "../lib/taskTypes";
import { confirmDialog } from "./DialogService";
import Builder from "./tasks/Builder";
import Detail from "./tasks/Detail";
import ScheduleCard from "./tasks/ScheduleCard";
import { Status } from "./tasks/shared";
import PageHeader from "../ui/PageHeader";
import "./TasksPage.css";
const ResearchMissions = lazy(() => import("./ResearchMissions"));

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
  const [view, setView] = useState(initialDiscoveryId ? "missions" : "active");
  const views = ["active", "scheduled", "history", "missions"];
  const tabList = useTabList(
    views,
    view,
    (id) => {
      // Only the Research view unmounts the builder; other views keep it.
      void (id === "missions" ? discardDraft() : Promise.resolve(true)).then(
        (ok) => {
          if (!ok) return;
          setView(id);
          setOffset(0);
        },
      );
    },
    "manual",
  );
  const [runs, setRuns] = useState<TaskSummary[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [selected, setSelected] = useState<TaskRun | null>(null);
  const selectedRef = useRef<string | null>(initialTaskId ?? null);
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
  const [error, setError] = useState("");
  const [background, setBackground] = useState(false);
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const refreshVersion = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++refreshVersion.current;
    const selectedId = selectedRef.current;
    try {
      const [rows, planned, detail] = await Promise.all([
        view === "missions"
          ? Promise.resolve([])
          : taskClient.list(view, null, offset),
        view === "scheduled" ? taskClient.schedules() : Promise.resolve([]),
        // Cleanup can remove a run a saved route still names; the lists stand.
        selectedId
          ? taskClient.get(selectedId).catch(() => null)
          : Promise.resolve(null),
      ]);
      if (version !== refreshVersion.current) return;
      setRuns(rows);
      setSchedules(planned);
      if (selectedId === selectedRef.current) {
        if (!detail) selectedRef.current = null;
        setSelected(detail);
      }
      setError("");
    } catch (e) {
      if (version === refreshVersion.current) setError(String(e));
    } finally {
      if (version === refreshVersion.current) setLoading(false);
    }
  }, [view, offset]);
  useEffect(() => {
    refreshVersion.current++;
    selectedRef.current = initialTaskId ?? null;
    setSelected(null);
    setCreating(
      Boolean((initialSessionId || initialAutomatePath) && !initialTaskId),
    );
    setView(initialDiscoveryId ? "missions" : "active");
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
    const unlisten = listen("tasks:changed", queue);
    const errors = listen<string>("tasks:error", (e) => setError(e.payload));
    return () => {
      disposed = true;
      refreshVersion.current++;
      if (timer) clearTimeout(timer);
      void unlisten.then((f) => f());
      void errors.then((f) => f());
    };
  }, [refresh]);
  useEffect(() => {
    void taskClient
      .background()
      .then(setBackground)
      .catch((e) => setError(String(e)));
  }, []);
  const select = (run: TaskRun) => {
    selectedRef.current = run.id;
    setSelected(run);
    setCreating(false);
    void refresh();
  };
  // The side pane renders at the top of its column (or below the list in the
  // narrow layout). Bring its heading into view when it opens from a scrolled
  // list; "nearest" leaves an already visible pane where it is.
  const paneKey = creating ? "new" : (selected?.id ?? null);
  useEffect(() => {
    if (!paneKey) return;
    const pane = contentRef.current?.querySelector(
      ":scope > .task-builder, :scope > .task-detail",
    );
    pane?.firstElementChild?.scrollIntoView?.({ block: "nearest" });
  }, [paneKey]);
  return (
    <main className="ui-page tasks-page">
      <PageHeader
        title="Automations"
        description="Run project work and reviews now or on a schedule, with pauses for your input."
        action={
          view !== "missions" && (
            <button
              className="task-primary"
              onClick={() => {
                setCreating(true);
                selectedRef.current = null;
                setSelected(null);
              }}
            >
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
              {v === "missions"
                ? "Research"
                : v.charAt(0).toUpperCase() + v.slice(1)}
            </button>
          ))}
        </div>
        <label className="task-background">
          <input
            type="checkbox"
            checked={background}
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
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      {views
        .filter((id) => id !== view)
        .map((id) => (
          <div key={id} {...tabList.panelProps(id)} hidden />
        ))}
      <div {...tabList.panelProps(view)}>
        {view === "missions" ? (
          <Suspense
            fallback={
              <p className="task-empty">Loading research automations…</p>
            }
          >
            <ResearchMissions
              initialDiscoveryId={initialDiscoveryId}
              initialSessionId={initialSessionId}
              onConversation={onConversation}
              onTask={async (id) => {
                const run = await taskClient.get(id);
                if (!(await discardDraft())) return;
                setView("active");
                select(run);
              }}
            />
          </Suspense>
        ) : (
          <div
            ref={contentRef}
            className={`tasks-content ${creating || selected ? "tasks-content--split" : ""}`}
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
              ) : runs.length ? (
                <>
                  {runs.map((run) => (
                    <button
                      className={`task-list-row ${selected?.id === run.id ? "is-selected" : ""}`}
                      key={run.id}
                      onClick={async () => {
                        if (!(await discardDraft())) return;
                        selectedRef.current = run.id;
                        setCreating(false);
                        void taskClient
                          .get(run.id)
                          .then((detail) => {
                            if (selectedRef.current === run.id)
                              setSelected(detail);
                          })
                          .catch((e) => {
                            if (selectedRef.current === run.id)
                              setError(String(e));
                          });
                      }}
                    >
                      <div>
                        <strong>{run.name}</strong>
                        <span>
                          {run.reason ||
                            (run.dueAt && run.dueAt > Date.now() / 1000
                              ? `Starts ${taskTime(run.dueAt)}`
                              : "Ready for the next step")}
                        </span>
                      </div>
                      <Status state={run.state} />
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
                    {runs.length === 50 && (
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
                      : selected
                        ? "No active automations."
                        : "Create your first automation."}
                  </h2>
                  <p>
                    Connect project conversations and reviews, pause for input,
                    or set a time to continue.
                  </p>
                  {view !== "history" && (
                    <button onClick={() => setCreating(true)}>
                      Create an automation →
                    </button>
                  )}
                </div>
              )}
            </section>
            {creating ? (
              <Builder
                key={`${entryRequest}:${initialSessionId ?? ""}:${initialAutomatePath ?? ""}`}
                onDirtyChange={handleDirtyChange}
                initialSessionId={initialSessionId}
                initialPath={initialAutomatePath}
                onPrepared={select}
                onClose={() =>
                  void discardDraft().then((ok) => {
                    if (ok) setCreating(false);
                  })
                }
              />
            ) : (
              selected && (
                <Detail
                  key={selected.id}
                  run={selected}
                  onUpdated={select}
                  onClose={() => {
                    selectedRef.current = null;
                    setSelected(null);
                  }}
                  onConversation={onConversation}
                />
              )
            )}
          </div>
        )}
      </div>
    </main>
  );
}
