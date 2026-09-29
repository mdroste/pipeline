import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  discoveryClient,
  discoveryNames,
  discoveryStatus,
  type DiscoveryMode,
  type DiscoveryRun,
  type DiscoverySummary,
} from "../../lib/discoveryClient";
import Builder from "./Builder";
import Detail from "./Detail";
import "./discovery.css";

export default function SelfDiscovery({
  initialRunId,
  initialMode = "supervised",
  initialSessionId,
  onBack,
  onConversation,
}: {
  initialRunId?: string | null;
  initialMode?: DiscoveryMode;
  initialSessionId?: string | null;
  onBack: () => void;
  onConversation?: (id: string) => void | Promise<void>;
}) {
  const [rows, setRows] = useState<DiscoverySummary[]>([]);
  const [run, setRun] = useState<DiscoveryRun | null>(null);
  const [creating, setCreating] = useState(!initialRunId);
  const [error, setError] = useState("");
  const [offset, setOffset] = useState(0);
  const selected = useRef<string | null>(initialRunId ?? null);
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    const version = ++generation.current;
    const id = selected.current;
    try {
      const [list, detail] = await Promise.all([
        discoveryClient.list(offset),
        id ? discoveryClient.get(id) : Promise.resolve(null),
      ]);
      if (version !== generation.current) return;
      setRows(list);
      if (id === selected.current) setRun(detail);
    } catch (e) {
      if (version === generation.current) setError(String(e));
    }
  }, [offset]);
  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void refresh();
    const pending = listen("discovery:changed", () => {
      if (live && !timer)
        timer = setTimeout(() => {
          timer = undefined;
          if (live) void refresh();
        }, 200);
    });
    return () => {
      live = false;
      generation.current++;
      if (timer) clearTimeout(timer);
      void pending.then((f) => f()).catch(() => undefined);
    };
  }, [refresh]);
  function update(r: DiscoveryRun) {
    selected.current = r.id;
    generation.current++;
    setRun(r);
    setCreating(false);
    void refresh();
  }
  return (
    <section
      className="research-missions discovery-page"
      aria-label="Full self-discovery"
    >
      <div className="task-section-heading">
        <div>
          <button onClick={onBack}>← Research automations</button>
          <h2>Full self-discovery</h2>
          <p className="task-muted">
            Develop original research across academic fields, from topic to
            assessed papers.
          </p>
        </div>
        <button
          onClick={() => {
            selected.current = null;
            generation.current++;
            setRun(null);
            setCreating(true);
          }}
        >
          New self-discovery
        </button>
      </div>
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      <div
        className={`tasks-content ${creating || run ? "tasks-content--split" : ""}`}
      >
        <section className="task-list" aria-label="Self-discovery history">
          {rows.map((r) => (
            <button
              className={`task-list-row ${run?.id === r.id ? "is-selected" : ""}`}
              key={r.id}
              onClick={() => {
                selected.current = r.id;
                setCreating(false);
                setRun(null);
                void refresh();
              }}
            >
              <div>
                <strong>{r.prompt}</strong>
                <span>{discoveryNames[r.mode]}</span>
                <small>{r.reason}</small>
              </div>
              <span className="task-status">{discoveryStatus(r.state)}</span>
            </button>
          ))}
          {!rows.length && (
            <p className="task-empty">
              Your research portfolios will appear here.
            </p>
          )}
          <div className="task-actions">
            {offset > 0 && (
              <button onClick={() => setOffset((n) => Math.max(0, n - 25))}>
                Previous
              </button>
            )}
            {rows.length === 25 && (
              <button onClick={() => setOffset((n) => n + 25)}>More</button>
            )}
          </div>
        </section>
        {creating ? (
          <Builder
            initialMode={initialMode}
            initialSessionId={initialSessionId}
            onStarted={update}
            onClose={() => setCreating(false)}
          />
        ) : (
          run && (
            <Detail
              key={run.id}
              run={run}
              onUpdated={(r) => {
                if (selected.current === r.id) update(r);
              }}
              onClose={() => {
                selected.current = null;
                generation.current++;
                setRun(null);
              }}
              onConversation={onConversation}
            />
          )
        )}
      </div>
    </section>
  );
}
