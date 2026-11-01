import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { taskClient } from "../lib/taskClient";
import { type TaskSummary } from "../lib/taskTypes";
import { runtimeClient } from "../lib/workbench/runtimeClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import { listedRunStatus } from "../lib/reportPresentation";
import type { RunSummary } from "../lib/types";
import type { WorkbenchEvent } from "../lib/workbenchTypes";
import { button, muted, Inspect } from "./research-programs/shared";
import PageHeader from "../ui/PageHeader";
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
    [error, setError] = useState(""),
    [reviewError, setReviewError] = useState("");
  const refresh = async () => {
    const [a, t] = await Promise.allSettled([
      invoke<Activity>("workbench_research_activity"),
      taskClient.list(),
    ]);
    if (a.status === "fulfilled") setActivity(a.value);
    if (t.status === "fulfilled") setTasks(t.value);
    setError(
      [a, t]
        .flatMap((result) =>
          result.status === "rejected"
            ? [workbenchErrorMessage(result.reason)]
            : [],
        )
        .join(" · "),
    );
  };
  const refreshReviews = () =>
    invoke<RunSummary[]>("list_runs")
      .then((next) => {
        setReviews(next);
        setReviewError("");
      })
      .catch((e) => setReviewError(workbenchErrorMessage(e)));
  useEffect(() => {
    void refresh().catch((e) => setError(workbenchErrorMessage(e)));
    void refreshReviews();
    let pending = false;
    const timer = setInterval(() => {
      if (pending || document.visibilityState === "hidden") return;
      pending = true;
      void refresh()
        .catch(() => undefined)
        .finally(() => {
          pending = false;
        });
    }, 4000);
    // Listing reviews reads every run, so it follows the queue. The slow timer
    // catches states the queue does not report, such as a paused review.
    const reviewTimer = setInterval(() => {
      if (document.visibilityState !== "hidden") void refreshReviews();
    }, 30_000);
    const queue = listen("runs:queue", () => void refreshReviews()).catch(
      () => undefined,
    );
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !asPage) onClose();
    };
    window.addEventListener("keydown", key);
    return () => {
      clearInterval(timer);
      clearInterval(reviewTimer);
      void queue.then((off) => off?.());
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
  const action = asPage ? "ui-page-button mr-2" : button;
  const row = asPage ? "ui-page-row space-y-2" : "space-y-2 border-t py-3";
  return (
    <aside
      role={asPage ? undefined : "dialog"}
      aria-label={asPage ? "Activity" : "Suite activity"}
      className={
        asPage
          ? "ui-page"
          : "absolute right-0 top-0 z-50 flex h-full w-[min(32rem,95vw)] flex-col overflow-auto border-l bg-white p-5 shadow-xl dark:bg-neutral-950"
      }
    >
      {asPage ? (
        <PageHeader
          title="Activity"
          description="See work in progress, completed work, and items that need your attention."
        />
      ) : (
        <div className="flex justify-between">
          <h2 className="font-semibold">Suite activity</h2>
          <button autoFocus className={button} onClick={onClose}>
            Close
          </button>
        </div>
      )}
      {(error || reviewError) && (
        <p role="alert">{[error, reviewError].filter(Boolean).join(" · ")}</p>
      )}
      <p className={`${muted} ${asPage ? "ui-page-toolbar" : "my-3"}`}>
        {activity?.pendingRequests.length ?? 0} unresolved conversation requests
        · {activity?.researchAttention ?? 0} research attention items
      </p>
      {activity?.turns.map((t) => (
        <section className={row} key={`${t.sessionId}:${t.turnId}`}>
          <h3>{t.title}</h3>
          <p className={muted}>Conversation · {t.state}</p>
          <button className={action} onClick={() => onSession(t.sessionId)}>
            Open conversation and requests
          </button>
          {t.turnId && (
            <button
              className={action}
              onClick={() =>
                void runtimeClient
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
        <section className={row} key={j.id}>
          <h3>
            {j.title} · {j.adapter}
          </h3>
          <p className={muted}>
            Local job · {j.state} · owner: {j.owner ?? "unestablished"}
          </p>
          <button className={action} onClick={() => onProject(j.workspaceId)}>
            Open job logs and recovery
          </button>
        </section>
      ))}
      {tasks
        .filter((t) => !["finished", "cancelled"].includes(t.state))
        .map((t) => (
          <section className={row} key={t.id}>
            <h3>{t.name}</h3>
            <p className={muted}>
              Automation · {t.state} · {t.reason}
            </p>
            <button className={action} onClick={() => onTasks(t.id)}>
              Open task controls
            </button>
          </section>
        ))}
      {reviews
        .filter((r) => ["running", "paused", "interrupted"].includes(r.status))
        .map((r) => (
          <section className={row} key={r.run_id}>
            <h3>{r.title || r.input_name}</h3>
            <p className={muted}>Review · {r.status}</p>
            <button className={action} onClick={() => onReview(r.run_id)}>
              Open Review controls
            </button>
          </section>
        ))}
      {asPage && running === 0 && (
        <p className={`${muted} ui-page-row`}>Nothing is running right now.</p>
      )}
      {asPage && (
        <section aria-label="Recently finished reviews" className="mt-7">
          <h2 className="ui-page-label">Recently finished reviews</h2>
          {reviews
            .filter(
              (r) => !["running", "paused", "interrupted"].includes(r.status),
            )
            .slice(0, 6)
            .map((r) => (
              <div
                key={r.run_id}
                className="ui-page-row flex items-center gap-3"
              >
                <span className="min-w-0 flex-1 truncate text-sm">
                  {r.title || r.input_name}
                </span>
                <span className={muted}>{listedRunStatus(r.status)}</span>
                <button
                  className="ui-page-button"
                  onClick={() => onReview(r.run_id)}
                >
                  Open
                </button>
              </div>
            ))}
          {reviews.length === 0 && (
            <p className={`${muted} px-3 pt-2`}>No reviews yet.</p>
          )}
        </section>
      )}
      <p className={`${muted} ${asPage ? "mt-6 px-3" : "mt-4"}`}>
        Stop or resume work from its own page. Stopping a review leaves project
        jobs running.
      </p>
    </aside>
  );
}
