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
import { Icon } from "../../ui/icons";
import Builder from "./Builder";
import Detail from "./Detail";
import "./discovery.css";
import "./setup.css";
import ReportIssueLink from "../ReportIssueLink";

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
  // Keep the brief mounted while the researcher browses earlier portfolios.
  const [hasDraft, setHasDraft] = useState(!initialRunId);
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
      if (id === selected.current)
        setRun((current) =>
          current?.id === detail?.id &&
          current &&
          detail &&
          current.revision > detail.revision
            ? current
            : detail,
        );
    } catch (e) {
      if (version === generation.current) setError(String(e));
    }
  }, [offset]);
  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void refresh();
    const queue = () => {
      if (live && !timer)
        timer = setTimeout(() => {
          timer = undefined;
          if (live) void refresh();
        }, 200);
    };
    // A step starting or ending moves the run's revision with only a task event.
    const pending = [
      listen("discovery:changed", queue),
      listen("tasks:changed", queue),
    ];
    const fallback = setInterval(queue, 10000);
    return () => {
      live = false;
      generation.current++;
      if (timer) clearTimeout(timer);
      clearInterval(fallback);
      for (const p of pending) void p.then((f) => f()).catch(() => undefined);
    };
  }, [refresh]);
  function update(r: DiscoveryRun) {
    selected.current = r.id;
    generation.current++;
    setRun((current) =>
      current?.id === r.id && current.revision > r.revision ? current : r,
    );
    setCreating(false);
    void refresh();
  }
  return (
    <section
      className="research-missions discovery-page"
      aria-label="Research discovery"
    >
      <nav className="ds-nav" aria-label="Research discovery navigation">
        <button onClick={onBack}>
          <Icon name="arrow-left" /> Research automations
        </button>
        <button
          onClick={() => {
            selected.current = null;
            generation.current++;
            setRun(null);
            if (creating) void refresh();
            else setHasDraft(true);
            setCreating(!creating);
          }}
        >
          <Icon
            name={creating ? "history" : hasDraft ? "arrow-left" : "plus"}
          />
          {creating
            ? "Portfolio history"
            : hasDraft
              ? "Return to setup"
              : "New research discovery"}
        </button>
      </nav>
      <header className="ds-heading">
        <h1>{creating ? "Research discovery" : "Portfolio history"}</h1>
        <p>
          {creating
            ? "Turn a research question into a shortlist of proposals and reviewed paper drafts."
            : "Open a research portfolio to see its progress and results."}
        </p>
      </header>
      {error && (
        <p className="task-error" role="alert">
          {error}
          <ReportIssueLink />
        </p>
      )}
      {hasDraft && (
        <div hidden={!creating}>
          <Builder
            initialMode={initialMode}
            initialSessionId={initialSessionId}
            onStarted={(r) => {
              setHasDraft(false);
              update(r);
            }}
          />
        </div>
      )}
      {!creating && (
        <div className={`tasks-content ${run ? "tasks-content--split" : ""}`}>
          <section
            className="task-list"
            aria-label="Research discovery history"
          >
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
          {run && (
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
          )}
        </div>
      )}
    </section>
  );
}
