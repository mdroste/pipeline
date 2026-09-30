import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { taskClient, type TaskSummary } from "../lib/taskClient";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type { RunSummary } from "../lib/types";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import { button, muted, Inspect } from "./research-programs/shared";
interface Activity {
  turns: {
    sessionId: string;
    workspaceId: string | null;
    title: string;
    threadId: string;
    turnId: string | null;
    state: string;
  }[];
  jobs: {
    id: string;
    workspaceId: string;
    title: string;
    adapter: string;
    state: string;
    owner: string | null;
  }[];
  pendingRequests: WorkbenchEvent[];
  researchAttention: number;
}
export default function ResearchActivity({
  onClose,
  onSession,
  onProject,
  onTasks,
  onReview,
  asPage = false,
}: {
  onClose: () => void;
  onSession: (id: string) => void;
  onProject: (id: string) => void;
  onTasks: (id: string) => void;
  onReview: (id: string) => void;
  /** Render as the Activity destination instead of a side overlay. */
  asPage?: boolean;
}) {
  const [activity, setActivity] = useState<Activity | null>(null),
    [tasks, setTasks] = useState<TaskSummary[]>([]),
    [reviews, setReviews] = useState<RunSummary[]>([]),
    [error, setError] = useState("");
  const refresh = async () => {
    const [a, t, r] = await Promise.allSettled([
      invoke<Activity>("workbench_research_activity"),
      taskClient.list(),
      invoke<RunSummary[]>("list_runs"),
    ]);
    if (a.status === "fulfilled") setActivity(a.value);
    if (t.status === "fulfilled") setTasks(t.value);
    if (r.status === "fulfilled") setReviews(r.value);
    setError(
      [a, t, r]
        .flatMap((result) =>
          result.status === "rejected"
            ? [workbenchErrorMessage(result.reason)]
            : [],
        )
        .join(" · "),
    );
  };
  useEffect(() => {
    void refresh().catch((e) => setError(workbenchErrorMessage(e)));
    const timer = setInterval(
      () => void refresh().catch(() => undefined),
      4000,
    );
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !asPage) onClose();
    };
    window.addEventListener("keydown", key);
    return () => {
      clearInterval(timer);
      window.removeEventListener("keydown", key);
    };
  }, []);
  const running =
    (activity?.turns.length ?? 0) +
    (activity?.jobs.length ?? 0) +
    tasks.filter((t) => !["finished", "cancelled"].includes(t.state)).length +
    reviews.filter((r) =>
      ["running", "paused", "interrupted"].includes(r.status),
    ).length;
  return (
    <aside
      role={asPage ? undefined : "dialog"}
      aria-label={asPage ? "Activity" : "Suite activity"}
      className={
        asPage
          ? "mx-auto flex w-full max-w-3xl flex-col px-6 py-8"
          : "absolute right-0 top-0 z-50 flex h-full w-[min(32rem,95vw)] flex-col overflow-auto border-l bg-white p-5 shadow-xl dark:bg-neutral-950"
      }
    >
      <div className="flex justify-between">
        {asPage ? (
          <div>
            <h1 className="text-xl font-semibold">Activity</h1>
            <p className={`${muted} mt-1`}>
              What is running, what needs you, and what finished — across
              conversations, reviews, automations, and local jobs.
            </p>
          </div>
        ) : (
          <>
            <h2 className="font-semibold">Suite activity</h2>
            <button autoFocus className={button} onClick={onClose}>
              Close
            </button>
          </>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
      <p className={`${muted} my-3`}>
        {activity?.pendingRequests.length ?? 0} unresolved conversation requests
        · {activity?.researchAttention ?? 0} research attention items
      </p>
      {activity?.turns.map((t) => (
        <section
          className="space-y-2 border-t py-3"
          key={`${t.sessionId}:${t.turnId}`}
        >
          <h3>{t.title}</h3>
          <p className={muted}>Conversation · {t.state}</p>
          <button className={button} onClick={() => onSession(t.sessionId)}>
            Open conversation and requests
          </button>
          {t.turnId && (
            <button
              className={button}
              onClick={() =>
                void workbenchClient
                  .interruptTurn(t.threadId, t.turnId!)
                  .then(refresh)
                  .catch((e) => setError(workbenchErrorMessage(e)))
              }
            >
              Stop this turn
            </button>
          )}
          <Inspect
            label="Pending requests for this turn"
            value={activity.pendingRequests.filter(
              (r) => r.params?.threadId === t.threadId,
            )}
          />
        </section>
      ))}
      {activity?.jobs.map((j) => (
        <section className="space-y-2 border-t py-3" key={j.id}>
          <h3>
            {j.title} · {j.adapter}
          </h3>
          <p className={muted}>
            Local job · {j.state} · owner: {j.owner ?? "unestablished"}
          </p>
          <button className={button} onClick={() => onProject(j.workspaceId)}>
            Open job logs and recovery
          </button>
        </section>
      ))}
      {tasks
        .filter((t) => !["finished", "cancelled"].includes(t.state))
        .map((t) => (
          <section className="space-y-2 border-t py-3" key={t.id}>
            <h3>{t.name}</h3>
            <p className={muted}>
              Automation · {t.state} · {t.reason}
            </p>
            <button className={button} onClick={() => onTasks(t.id)}>
              Open task controls
            </button>
          </section>
        ))}
      {reviews
        .filter((r) => ["running", "paused", "interrupted"].includes(r.status))
        .map((r) => (
          <section className="space-y-2 border-t py-3" key={r.run_id}>
            <h3>{r.title || r.input_name}</h3>
            <p className={muted}>Review · {r.status}</p>
            <button className={button} onClick={() => onReview(r.run_id)}>
              Open Review controls
            </button>
          </section>
        ))}
      {asPage && running === 0 && (
        <p className={`${muted} border-t py-4`}>
          Nothing is running right now.
        </p>
      )}
      {asPage && (
        <section aria-label="Recently finished reviews" className="mt-6">
          <h2 className="text-sm font-semibold">Recently finished reviews</h2>
          {reviews
            .filter(
              (r) => !["running", "paused", "interrupted"].includes(r.status),
            )
            .slice(0, 6)
            .map((r) => (
              <div
                key={r.run_id}
                className="flex items-center gap-3 border-t py-2.5 first:border-t-0"
              >
                <span className="min-w-0 flex-1 truncate text-sm">
                  {r.title || r.input_name}
                </span>
                <span className={muted}>{r.status}</span>
                <button className={button} onClick={() => onReview(r.run_id)}>
                  Open
                </button>
              </div>
            ))}
          {reviews.length === 0 && (
            <p className={`${muted} mt-2`}>No reviews yet.</p>
          )}
        </section>
      )}
      <p className={`${muted} mt-4`}>
        Each runtime retains its own Stop and recovery controls. Stopping a
        Stopping a review does not cancel project jobs.
      </p>
    </aside>
  );
}
