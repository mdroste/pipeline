import { useEffect, useRef, useState } from "react";
import {
  discoveryClient,
  discoveryTerminal,
  type DiscoveryActivity,
  type DiscoveryRun,
} from "../../lib/discoveryClient";

function duration(seconds: number) {
  const value = Math.max(0, Math.floor(seconds));
  if (value < 60) return `${value}s`;
  if (value < 3600) return `${Math.floor(value / 60)}m ${value % 60}s`;
  return `${Math.floor(value / 3600)}h ${Math.floor((value % 3600) / 60)}m`;
}

const status = (state: string) =>
  ({
    queued: "Waiting to start",
    starting: "Starting",
    running: "Running",
    inProgress: "Running",
    completed: "Completed",
    finished: "Completed",
    recorded: "Recorded",
    failed: "Failed",
    cancelled: "Stopped",
    interrupted: "Interrupted",
    unknown: "Outcome uncertain",
    attention: "Needs attention",
    paused: "Paused",
  })[state] ?? "Status unavailable";

export default function Activity({
  run,
  onUpdated,
}: {
  run: DiscoveryRun;
  onUpdated: (run: DiscoveryRun) => void;
}) {
  const [activity, setActivity] = useState<DiscoveryActivity | null>(null);
  const [error, setError] = useState("");
  const [clock, setClock] = useState(Date.now());
  const clockOffset = useRef(0);
  const update = useRef(onUpdated);
  update.current = onUpdated;
  const polling =
    !discoveryTerminal(run.state) &&
    (run.state === "running" ||
      run.state === "stopping" ||
      Boolean(run.activeChild));

  useEffect(() => {
    let live = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    async function refresh() {
      try {
        const next = await discoveryClient.activity(run.id);
        if (!live) return;
        clockOffset.current = next.observedAt * 1000 - Date.now();
        setClock(Date.now());
        setActivity(next);
        setError("");
        if (next.revision > run.revision) {
          const fresh = await discoveryClient.get(run.id);
          if (live) update.current(fresh);
        }
      } catch (e) {
        if (live) setError(`Activity updates unavailable. ${String(e)}`);
      } finally {
        if (live && polling) timer = setTimeout(() => void refresh(), 2000);
      }
    }
    void refresh();
    return () => {
      live = false;
      if (timer) clearTimeout(timer);
    };
  }, [run.id, run.revision, polling]);

  useEffect(() => {
    const timer = setInterval(() => setClock(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  const now = (clock + clockOffset.current) / 1000;
  const step = activity?.step;
  const completed =
    activity?.agents.filter(
      (a) => a.label.startsWith("Idea agent ") && a.state === "completed",
    ).length ?? 0;
  const hasAgents = activity?.agents.some((a) =>
    a.label.startsWith("Idea agent "),
  );
  return (
    <section className="discovery-activity" aria-label="Live research activity">
      <div className="discovery-clocks">
        <p>
          Last activity{" "}
          {activity?.lastActivityAt != null ? (
            <time
              dateTime={new Date(activity.lastActivityAt * 1000).toISOString()}
              title={new Date(activity.lastActivityAt * 1000).toLocaleString()}
            >
              {duration(now - activity.lastActivityAt)} ago
            </time>
          ) : (
            "awaiting an update"
          )}
        </p>
        {step && (
          <p>
            Current step{" "}
            {step.startedAt != null &&
            (step.finishedAt != null ||
              ["running", "inProgress"].includes(step.state)) ? (
              <>
                <strong>
                  {duration((step.finishedAt ?? now) - step.startedAt)}
                </strong>{" "}
                · {status(step.state)}
              </>
            ) : (
              <>
                {step.startedAt != null && "Elapsed time unavailable · "}
                {status(step.state)}
              </>
            )}
          </p>
        )}
      </div>
      {error && (
        <p className="task-error" role="status">
          {error}
        </p>
      )}
      {Boolean(run.definition.ideaAgents) && (
        <p className="task-muted">
          Idea agents · {hasAgents ? completed : (run.completedIdeaAgents ?? 0)}{" "}
          of {run.definition.ideaAgents} completed
        </p>
      )}
      {!!activity?.agents.length && (
        <ul className="discovery-agents" aria-label="Agent status">
          {activity.agents.map((agent) => (
            <li key={agent.id}>
              <span>{agent.label}</span>
              <span data-state={agent.state}>{status(agent.state)}</span>
            </li>
          ))}
        </ul>
      )}
      <details open>
        <summary>Activity feed</summary>
        {activity?.entries.length ? (
          <ol className="discovery-feed">
            {activity.entries.map((entry) => (
              <li key={entry.id}>
                <time dateTime={new Date(entry.at * 1000).toISOString()}>
                  {new Date(entry.at * 1000).toLocaleTimeString()}
                </time>
                <span>{entry.label}</span>
                {entry.state !== "recorded" && (
                  <small>{status(entry.state)}</small>
                )}
              </li>
            ))}
          </ol>
        ) : (
          <p className="task-muted">Waiting for recorded activity.</p>
        )}
      </details>
    </section>
  );
}
