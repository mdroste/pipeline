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
import {
  taskClient,
  taskTime,
  type Schedule,
  type TaskRun,
  type TaskSummary,
} from "../lib/taskClient";
import Builder from "./tasks/Builder";
import Detail from "./tasks/Detail";
import ScheduleCard from "./tasks/ScheduleCard";
import { Status } from "./tasks/shared";
import "./TasksPage.css";
const ResearchMissions = lazy(() => import("./ResearchMissions"));

export default function TasksPage({
  initialSessionId,
  onConversation,
  initialTaskId,
  initialDiscoveryId,
}: {
  initialSessionId?: string | null;
  onConversation?: (id: string) => void | Promise<void>;
  initialTaskId?: string | null;
  initialDiscoveryId?: string | null;
}) {
  const [view, setView] = useState(initialDiscoveryId ? "missions" : "active");
  const views = ["active", "scheduled", "history", "missions"];
  const tabList = useTabList(
    views,
    view,
    (id) => {
      setView(id);
      setOffset(0);
    },
    "manual",
  );
  const [runs, setRuns] = useState<TaskSummary[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [selected, setSelected] = useState<TaskRun | null>(null);
  const selectedRef = useRef<string | null>(initialTaskId ?? null);
  const [creating, setCreating] = useState(
    Boolean(initialSessionId && !initialTaskId),
  );
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
        selectedId ? taskClient.get(selectedId) : Promise.resolve(null),
      ]);
      if (version !== refreshVersion.current) return;
      setRuns(rows);
      setSchedules(planned);
      if (selectedId === selectedRef.current) setSelected(detail);
      setError("");
    } catch (e) {
      if (version === refreshVersion.current) setError(String(e));
    } finally {
      if (version === refreshVersion.current) setLoading(false);
    }
  }, [view, offset]);
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
    localStorage.setItem("pipeline.tasks.enabled", "true");
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
  return (
    <main className="tasks-page">
      <header className="tasks-header">
        <div>
          <p className="task-eyebrow">PIPELINE</p>
          <h1>Automations</h1>
          <p className="task-muted">
            Continue project work, reviews, waits, and decisions now or on a
            schedule.
          </p>
        </div>
        {view !== "missions" && (
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
        )}
      </header>
      <div className="tasks-toolbar">
        <div className="task-tabs" role="tablist" aria-label="Automations view">
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
                setView("active");
                select(run);
              }}
            />
          </Suspense>
        ) : (
          <div
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
                      onClick={() => {
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
                        : "Give your work a next step."}
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
                initialSessionId={initialSessionId}
                onPrepared={select}
                onClose={() => setCreating(false)}
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
