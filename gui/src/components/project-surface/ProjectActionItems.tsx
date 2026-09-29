import { useState, useEffect, useRef } from "react";
import { projectClient, type ResearchTask } from "../../lib/projectClient";
import {
  TASK_STATUSES,
  button,
  input,
  muted,
  linkButton,
  isOpenTask,
  type SurfaceApi,
} from "./shared";
export default function ProjectActionItems(props: SurfaceApi) {
  const {
    data,
    workspace,
    workspaceId,
    run,
    setTaskId,
    setTab,
    openAnnotation,
    act,
  } = props;
  const [objective, setObjective] = useState("");
  const [expectedOutputs, setExpectedOutputs] = useState("");
  const [expectedChecks, setExpectedChecks] = useState("");
  const [history, setHistory] = useState(data.tasks);
  const [cursor, setCursor] = useState(data.tasksCursor);
  const [pageError, setPageError] = useState("");
  const [loading, setLoading] = useState(false);
  const current = useRef(data.tasks);
  current.current = data.tasks;
  useEffect(() => {
    setHistory(data.tasks);
    setCursor(data.tasksCursor);
    setPageError("");
  }, [data.tasks, data.tasksCursor]);
  const earlier = async () => {
    if (!cursor || loading) return;
    const base = data.tasks;
    setLoading(true);
    setPageError("");
    try {
      const page = await projectClient.taskPage(workspaceId, cursor);
      if (current.current !== base) return;
      setHistory((old) => [
        ...old,
        ...page.records.filter(
          (t) => !old.some((existing) => existing.id === t.id),
        ),
      ]);
      setCursor(page.nextCursor);
    } catch (e) {
      if (current.current === base) setPageError(String(e));
    } finally {
      setLoading(false);
    }
  };
  const canEdit = Boolean(workspace?.root) && data.fileAcceptance;
  return (
    <section className="space-y-3">
      <h2 className="font-semibold">Action items</h2>
      <div className="flex flex-wrap gap-2 sm:flex-nowrap">
        <textarea
          aria-label="New action item"
          value={objective}
          onChange={(e) => setObjective(e.target.value)}
          rows={2}
          className={input}
          placeholder="What needs doing?"
        />
        <button
          className={button}
          disabled={!objective.trim()}
          onClick={() =>
            void run(async () => {
              const task = await projectClient.mutate<ResearchTask>(
                workspaceId,
                {
                  action: "createTask",
                  objective,
                  anchorId: null,
                  expectedOutputs: expectedOutputs
                    .split("\n")
                    .map((s) => s.trim())
                    .filter(Boolean),
                  expectedChecks: expectedChecks
                    .split("\n")
                    .map((s) => s.trim())
                    .filter(Boolean),
                },
              );
              setTaskId(task.id);
              setObjective("");
              setExpectedOutputs("");
              setExpectedChecks("");
            })
          }
        >
          Add action item
        </button>
      </div>
      <details>
        <summary className={`cursor-pointer ${muted}`}>
          Intended outputs and checks (optional)
        </summary>
        <div className="mt-2 grid gap-3 sm:grid-cols-2">
          <textarea
            aria-label="Intended outputs"
            value={expectedOutputs}
            onChange={(e) => setExpectedOutputs(e.target.value)}
            className={input}
            rows={3}
            placeholder="Files or results this action item should produce, one per line"
          />
          <textarea
            aria-label="Checks to run"
            value={expectedChecks}
            onChange={(e) => setExpectedChecks(e.target.value)}
            className={input}
            rows={3}
            placeholder="How to check the result, one per line"
          />
        </div>
      </details>
      {!history.length && <p className={muted}>No action items yet.</p>}
      {history.map((t) => (
        <div
          key={t.id}
          className="flex flex-wrap items-center gap-3 rounded-lg border border-gray-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-950"
        >
          <span
            className={`min-w-0 flex-1 text-sm ${isOpenTask(t.body.status) ? "" : "text-gray-500 line-through decoration-gray-300"}`}
          >
            {t.body.objective}
          </span>
          {t.body.anchorId && (
            <button
              className={linkButton}
              onClick={() => {
                const anchor = data.anchors.find(
                  (a) => a.id === t.body.anchorId,
                );
                if (anchor) openAnnotation(anchor);
              }}
            >
              Show passage
            </button>
          )}
          {canEdit && isOpenTask(t.body.status) && (
            <button
              className={linkButton}
              onClick={() => {
                setTaskId(t.id, t);
                setTab("edits");
              }}
            >
              Start an edit
            </button>
          )}
          <select
            aria-label={`Status: ${t.body.objective}`}
            className="rounded-md border border-gray-300 bg-transparent p-1 text-xs dark:border-neutral-700"
            value={t.body.status}
            onChange={(e) =>
              void act({
                action: "updateTask",
                taskId: t.id,
                expectedRevision: t.revision,
                status: e.target.value,
              })
            }
          >
            {TASK_STATUSES.map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </div>
      ))}
      {pageError && <p role="alert">{pageError}</p>}
      {cursor && (
        <button
          className={button}
          disabled={loading}
          onClick={() => void earlier()}
        >
          {loading ? "Loading…" : "Show older action items"}
        </button>
      )}
    </section>
  );
}
