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
  missionClient,
  missionState,
  type Mission,
  type MissionSummary,
} from "../lib/missionClient";
import "./research-missions/Missions.css";
const Builder = lazy(() => import("./research-missions/Builder"));
const Detail = lazy(() => import("./research-missions/Detail"));
const SelfDiscovery = lazy(() => import("./self-discovery/Page"));

export default function ResearchMissions({
  initialDiscoveryId,
  initialSessionId,
  onConversation,
  onTask,
}: {
  initialDiscoveryId?: string | null;
  initialSessionId?: string | null;
  onConversation?: (id: string) => void | Promise<void>;
  onTask?: (id: string) => void | Promise<void>;
}) {
  const [rows, setRows] = useState<MissionSummary[]>([]);
  const [discovery, setDiscovery] = useState<
    "supervised" | "unsupervised" | null
  >(initialDiscoveryId ? "supervised" : null);
  const [mission, setMission] = useState<Mission | null>(null);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [offset, setOffset] = useState(0);
  const selected = useRef<string | null>(null);
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++generation.current;
    const id = selected.current;
    try {
      const [list, detail] = await Promise.all([
        missionClient.list(offset),
        id ? missionClient.get(id) : Promise.resolve(null),
      ]);
      if (version !== generation.current) return;
      setRows(list);
      if (id === selected.current) setMission(detail);
    } catch (e) {
      if (version === generation.current) setError(String(e));
    } finally {
      if (version === generation.current) setLoading(false);
    }
  }, [offset]);
  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const queue = () => {
      if (live && !timer)
        timer = setTimeout(() => {
          timer = undefined;
          if (live) void refresh();
        }, 180);
    };
    void refresh();
    const pending = [
      listen("missions:changed", queue),
      listen("tasks:changed", queue),
    ];
    return () => {
      live = false;
      generation.current++;
      if (timer) clearTimeout(timer);
      for (const p of pending) void p.then((f) => f()).catch(() => undefined);
    };
  }, [refresh]);
  function update(m: Mission) {
    if (selected.current && selected.current !== m.id && !creating) return;
    selected.current = m.id;
    setMission(m);
    setCreating(false);
    void refresh();
  }
  async function select(id: string) {
    selected.current = id;
    generation.current++;
    setCreating(false);
    setMission(null);
    setError("");
    try {
      const m = await missionClient.get(id);
      if (selected.current === id) setMission(m);
    } catch (e) {
      if (selected.current === id) setError(String(e));
    }
  }
  const create = () => {
    generation.current++;
    selected.current = null;
    setMission(null);
    setCreating(true);
  };
  if (discovery)
    return (
      <Suspense
        fallback={<p className="task-empty">Loading self-discovery…</p>}
      >
        <SelfDiscovery
          initialRunId={initialDiscoveryId}
          initialMode={discovery}
          initialSessionId={initialSessionId}
          onBack={() => setDiscovery(null)}
          onConversation={onConversation}
        />
      </Suspense>
    );
  return (
    <section className="research-missions" aria-label="Research automations">
      <div className="task-section-heading mission-header">
        <div>
          <h2>Research automations</h2>
          <p className="task-muted">
            Pursue a research question over several checked rounds.
          </p>
        </div>
        <button className="task-primary" onClick={create}>
          + New research automation
        </button>
      </div>
      <div className="task-actions" aria-label="Autonomous paper discovery">
        <button onClick={() => setDiscovery("supervised")}>
          Full self-discovery (supervised)
        </button>
        <button onClick={() => setDiscovery("unsupervised")}>
          Full self-discovery (unsupervised)
        </button>
      </div>
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      <div
        className={`tasks-content ${creating || mission ? "tasks-content--split" : ""}`}
      >
        <section className="task-list" aria-label="Research automation list">
          {loading ? (
            <p className="task-empty">Loading research automations…</p>
          ) : rows.length ? (
            <>
              {rows.map((m) => (
                <button
                  className={`task-list-row ${mission?.id === m.id ? "is-selected" : ""}`}
                  key={m.id}
                  onClick={() => void select(m.id)}
                >
                  <div>
                    <strong>{m.name}</strong>
                    <span>{m.reason}</span>
                    <small>
                      {m.rounds} rounds · {m.openQuestions} pending decisions
                    </small>
                  </div>
                  <span className={`task-status task-status--${m.state}`}>
                    {missionState(m.state)}
                  </span>
                </button>
              ))}
              <div className="task-actions">
                {offset > 0 && (
                  <button onClick={() => setOffset(Math.max(0, offset - 50))}>
                    Previous
                  </button>
                )}
                {rows.length === 50 && (
                  <button onClick={() => setOffset(offset + 50)}>More</button>
                )}
              </div>
            </>
          ) : (
            <div className="task-empty">
              <h3>Start autonomous research</h3>
              <p>
                Set a research question, budget, and completion criteria.
                Pipeline investigates, checks its findings, and prepares a
                report.
              </p>
              <button onClick={create}>Create a research automation →</button>
            </div>
          )}
        </section>
        <Suspense
          fallback={<p className="task-empty">Loading research automation…</p>}
        >
          {creating ? (
            <Builder
              initialSessionId={initialSessionId}
              onPrepared={(m) => {
                selected.current = m.id;
                update(m);
              }}
              onClose={() => setCreating(false)}
            />
          ) : (
            mission && (
              <Detail
                key={mission.id}
                mission={mission}
                onUpdated={update}
                onClose={() => {
                  generation.current++;
                  selected.current = null;
                  setMission(null);
                }}
                onConversation={onConversation}
                onTask={onTask}
              />
            )
          )}
        </Suspense>
      </div>
    </section>
  );
}
