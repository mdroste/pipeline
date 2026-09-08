import { useEffect, useRef, useState } from "react";
import { deskClient, operation, type ContextItem } from "../../lib/deskClient";
import {
  programClient,
  type Followup,
  type FollowupContextReview,
} from "../../lib/programClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { button, input, muted, Field, Inspect } from "./shared";
interface Props {
  sessionId: string;
  active: boolean;
  model: string;
  effort: string;
  onError: (s: string) => void;
  onDispatch: (busy: boolean) => void;
  onRefresh: () => Promise<void>;
  onBranch: (id: string) => Promise<void>;
}
export default function Followups({
  sessionId,
  active,
  model,
  effort,
  onError,
  onDispatch,
  onRefresh,
  onBranch,
}: Props) {
  const cache = `pipeline.nextFollowup.${sessionId}`;
  const [draft, setDraft] = useState(() => {
      try {
        return localStorage.getItem(cache) ?? "";
      } catch {
        return "";
      }
    }),
    [rows, setRows] = useState<Followup[]>([]),
    [busy, setBusy] = useState(false),
    [context, setContext] = useState<ContextItem[] | null>(null),
    [selected, setSelected] = useState<number[]>([]),
    [title, setTitle] = useState("New conversation");
  const guard = useRef(false);
  const [review, setReview] = useState<FollowupContextReview | null>(null);
  const [history, setHistory] = useState({ sessionId, offset: 0 });
  const historyOffset = history.sessionId === sessionId ? history.offset : 0;
  const listScope = useRef({ sessionId, historyOffset });
  listScope.current = { sessionId, historyOffset };
  const refresh = async () => {
    const next = await programClient.queue(sessionId, {
      action: "list",
      historyOffset,
    });
    if (
      listScope.current.sessionId === sessionId &&
      listScope.current.historyOffset === historyOffset
    )
      setRows(next);
  };
  useEffect(() => {
    setReview(null);
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [sessionId, active, historyOffset]);
  const run = async (fn: () => Promise<unknown>) => {
    if (guard.current) return;
    guard.current = true;
    setBusy(true);
    try {
      await fn();
      await refresh();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      guard.current = false;
      setBusy(false);
    }
  };
  const edit = (v: string) => {
    setDraft(v);
    try {
      if (v) localStorage.setItem(cache, v);
      else localStorage.removeItem(cache);
    } catch {
      /* The explicit Queue action persists to the research store. */
    }
  };
  return (
    <details className="space-y-2 border-t px-3 py-2 text-xs">
      <summary>
        Follow-up messages
        {rows.some(
          (r) => !["completed", "failed", "cancelled"].includes(r.state),
        )
          ? ` · ${rows.filter((r) => !["completed", "failed", "cancelled"].includes(r.state)).length} pending`
          : ""}
      </summary>
      <p className={muted}>
        Draft a follow-up while the assistant works. Queue next message saves
        it; Run next sends it after checking that the selected sources are still
        current.
      </p>
      <textarea
        className={input}
        aria-label="Next message draft"
        rows={3}
        value={draft}
        onChange={(e) => edit(e.target.value)}
        placeholder="The next research question…"
      />
      <button
        className={button}
        disabled={busy || !draft.trim()}
        onClick={() =>
          void run(async () => {
            await programClient.queue(sessionId, {
              action: "enqueue",
              text: draft,
              model: model || null,
              effort: effort || null,
              operationId: operation(),
            });
            edit("");
          })
        }
      >
        Queue next message
      </button>
      {rows.map((r) => (
        <div className="space-y-2 rounded border p-2" key={r.id}>
          <p className="whitespace-pre-wrap">{r.request.text}</p>
          <p className={muted}>{r.state} · budget: one turn</p>
          <div className="flex flex-wrap gap-1">
            {r.state === "queued" && (
              <button
                className={button}
                disabled={busy || active}
                onClick={() =>
                  void run(async () => {
                    const value =
                      await programClient.queue<FollowupContextReview>(
                        sessionId,
                        {
                          action: "reviewContext",
                          id: r.id,
                          revision: r.revision,
                        },
                      );
                    if (listScope.current.sessionId === sessionId)
                      setReview(value);
                  })
                }
              >
                Review current context
              </button>
            )}
            {r.state === "queued" && (
              <button
                className={button}
                disabled={busy || active}
                onClick={() =>
                  void run(async () => {
                    onDispatch(true);
                    try {
                      await programClient.queue(sessionId, {
                        action: "run",
                        id: r.id,
                        fingerprint: r.fingerprint,
                      });
                      await onRefresh();
                    } finally {
                      onDispatch(false);
                    }
                  })
                }
              >
                Run next
              </button>
            )}
            {r.state === "queued" &&
              ["up", "down", "cancel"].map((control) => (
                <button
                  className={button}
                  key={control}
                  disabled={busy}
                  onClick={() =>
                    void run(() =>
                      programClient.queue(sessionId, {
                        action: "control",
                        id: r.id,
                        revision: r.revision,
                        control,
                      }),
                    )
                  }
                >
                  {control === "up"
                    ? "Move up"
                    : control === "down"
                      ? "Move down"
                      : "Cancel queued message"}
                </button>
              ))}
            {["dispatching", "running", "attention"].includes(r.state) && (
              <button
                className={button}
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    await programClient.queue(sessionId, {
                      action: "reconcile",
                      id: r.id,
                    });
                    await onRefresh();
                  })
                }
              >
                Check message status
              </button>
            )}
            <button className={button} onClick={() => edit(r.request.text)}>
              Copy to next draft
            </button>
          </div>
          {r.state === "queued" &&
            review?.id === r.id &&
            review.revision === r.revision && (
              <section
                className="space-y-2 rounded border p-2"
                aria-label="Review follow-up context"
              >
                <p>
                  Conversation: {review.conversationTitle} · Project:{" "}
                  {review.workspaceName ?? "Unfiled"}
                </p>
                <p className="whitespace-pre-wrap">{review.request.text}</p>
                <p>
                  Model: {review.request.model ?? "Conversation default"} ·
                  Effort: {review.request.effort ?? "Default"}
                </p>
                {review.conversationChanged && (
                  <p>
                    The conversation has newer messages. Review them before
                    continuing this follow-up.
                  </p>
                )}
                {review.settingsChanged && (
                  <p>The research settings have changed.</p>
                )}
                {review.previousRoot !== review.request.binding.runtimeRoot && (
                  <p>Previous working folder: {review.previousRoot}</p>
                )}
                <p>Working folder: {review.request.binding.runtimeRoot}</p>
                <p>
                  Research preset: {review.settings.preset} · Permissions:{" "}
                  {review.settings.permissions} · Command network:{" "}
                  {review.settings.commandNetwork ? "allowed" : "off"}
                </p>
                <Inspect
                  label="Current research settings"
                  value={review.settings}
                />
                <Inspect
                  label="Previously selected sources"
                  value={review.previousContext.items}
                />
                <Inspect
                  label="Current selected sources"
                  value={review.request.context.items}
                />
                <p>
                  This updates the queued message. Run next remains a separate
                  action.
                </p>
                <button
                  className={button}
                  disabled={busy || active}
                  onClick={() =>
                    void run(async () => {
                      await programClient.queue(sessionId, {
                        action: "refreshContext",
                        id: review.id,
                        revision: review.revision,
                        fingerprint: review.fingerprint,
                      });
                      setReview(null);
                    })
                  }
                >
                  Use reviewed context
                </button>
                <button className={button} onClick={() => setReview(null)}>
                  Close review
                </button>
              </section>
            )}
          <Inspect
            value={{ context: r.request.context, result: r.result }}
            label="Sources and result"
          />
        </div>
      ))}
      <div className="flex gap-2">
        <button
          className={button}
          disabled={busy || historyOffset === 0}
          onClick={() =>
            setHistory({ sessionId, offset: Math.max(0, historyOffset - 100) })
          }
        >
          Newer history
        </button>
        <button
          className={button}
          disabled={
            busy ||
            rows.filter((r) =>
              ["completed", "failed", "cancelled"].includes(r.state),
            ).length < 100
          }
          onClick={() => setHistory({ sessionId, offset: historyOffset + 100 })}
        >
          Older history
        </button>
      </div>
      <details className="space-y-2">
        <summary>New conversation from selected context</summary>
        <p className={muted}>
          Start a new conversation with the selected sources. Conversation
          history and task permissions are not copied.
        </p>
        <button
          className={button}
          disabled={busy || active}
          onClick={() =>
            void run(async () => {
              const c = await deskClient.context(sessionId);
              setContext(c.items);
              setSelected(c.items.map((_, i) => i));
            })
          }
        >
          Preview selected context
        </button>
        {context && (
          <>
            <Field label="Conversation title">
              <input
                className={input}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </Field>
            {context.map((c, i) => (
              <label className="block" key={i}>
                <input
                  type="checkbox"
                  checked={selected.includes(i)}
                  onChange={(e) =>
                    setSelected((old) =>
                      e.target.checked
                        ? [...old, i]
                        : old.filter((n) => n !== i),
                    )
                  }
                />
                {c.role}: {c.object.kind} · {c.object.revision.slice(0, 10)}
              </label>
            ))}
            {!context.length && (
              <p className={muted}>
                Select research objects in the conversation context tray first.
              </p>
            )}
            <button
              className={button}
              disabled={busy || active || !selected.length}
              onClick={() =>
                void run(async () => {
                  const result = await programClient.queue<{
                    session: { id: string };
                  }>(sessionId, {
                    action: "branch",
                    title,
                    items: context.filter((_, i) => selected.includes(i)),
                    operationId: operation(),
                  });
                  await onBranch(result.session.id);
                })
              }
            >
              Create context branch
            </button>
          </>
        )}
      </details>
    </details>
  );
}
