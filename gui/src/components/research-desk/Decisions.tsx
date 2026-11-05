import { Fold } from "../../ui";
import { useCallback, useEffect, useState } from "react";
import {
  deskClient,
  reference,
  type DeskRecord,
  type Decision,
  type ContextItem,
  type ImpactReport,
} from "../../lib/deskClient";
import { researchClient } from "../../lib/workbench/researchClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import { projectClient } from "../../lib/projectClient";
import type { ResearchNote } from "../../lib/workbenchTypes";
import useReopenCount from "../../hooks/useReopenCount";
import { key } from "../research-programs/shared";
import { button, input, card, muted, type DeskProps } from "./shared";

/** Stays the same when a refresh reorders the conversation's sources. */
const sourceKey = ({ object }: ContextItem) => key(object);

export default function Decisions({
  workspaceId,
  sessionId,
  onOpen,
  onError,
  onRefresh,
  onActionStart,
  active,
}: DeskProps) {
  const [records, setRecords] = useState<DeskRecord<Decision>[]>([]);
  const [impact, setImpact] = useState<ImpactReport | null>(null);
  const [context, setContext] = useState("");
  const [sources, setSources] = useState<ContextItem[]>([]);
  const [statement, setStatement] = useState("");
  const [rationale, setRationale] = useState("");
  const [alternatives, setAlternatives] = useState("");
  const [state, setState] = useState<Decision["state"]>("proposed");
  const [prior, setPrior] = useState("");
  const [busy, setBusy] = useState(false);
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [reason, setReason] = useState("");
  const [handoff, setHandoff] = useState<DeskRecord | null>(null);
  const refresh = useCallback(async () => {
    const [r, i, h, s] = await Promise.all([
      deskClient.records<Decision>(workspaceId, "decision"),
      deskClient.impact(workspaceId),
      projectClient.home(workspaceId),
      sessionId
        ? deskClient.context(sessionId)
        : Promise.resolve({ items: [] }),
    ]);
    setRecords(r);
    setImpact(i);
    setContext(h.contextPreview);
    setSources(s.items);
  }, [workspaceId, sessionId]);
  const reopened = useReopenCount(active);
  useEffect(() => {
    void refresh().catch((e) => onError(workbenchErrorMessage(e)));
  }, [refresh, onError, reopened]);
  const run = async (fn: () => Promise<unknown>) => {
    if (busy) return;
    setBusy(true);
    onActionStart?.();
    try {
      await fn();
      await refresh();
      await onRefresh?.();
    } catch (e) {
      onError(workbenchErrorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  const fromSource = sources.find((s) => sourceKey(s) === from);
  const toSource = sources.find((s) => sourceKey(s) === to);
  const proposals = Array.isArray(handoff?.body.decisions)
    ? (handoff.body.decisions as ResearchNote[])
    : [];
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Decisions and rationale</h2>
        <textarea
          aria-label="Decision statement"
          className={input}
          value={statement}
          onChange={(e) => setStatement(e.target.value)}
          placeholder="Decision or rejected approach"
        />
        <textarea
          aria-label="Decision rationale"
          className={input}
          value={rationale}
          onChange={(e) => setRationale(e.target.value)}
          placeholder="Explain the decision, its evidence, and any remaining concerns."
        />
        <textarea
          aria-label="Decision alternatives"
          className={input}
          value={alternatives}
          onChange={(e) => setAlternatives(e.target.value)}
          placeholder="Alternatives considered, one per line"
        />
        <select
          aria-label="Decision state"
          className={input}
          value={state}
          onChange={(e) => setState(e.target.value as Decision["state"])}
        >
          {["proposed", "accepted", "rejected"].map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
        <select
          aria-label="Superseding decision"
          className={input}
          value={prior}
          onChange={(e) => setPrior(e.target.value)}
        >
          <option value="">Independent decision</option>
          {records.map((r) => (
            <option key={r.id} value={r.id}>
              Supersedes {r.title}
            </option>
          ))}
        </select>
        <p className={muted}>
          This decision links to the conversation’s selected sources. Accepted
          decisions appear in the project summary.
        </p>
        <button
          className={button}
          disabled={busy || !statement.trim() || !rationale.trim()}
          onClick={() =>
            void run(() =>
              deskClient.decision(
                workspaceId,
                statement.slice(0, 160),
                {
                  statement,
                  rationale,
                  alternatives: alternatives.split("\n").filter(Boolean),
                  assumptions: sources.map((i) => i.object),
                  state,
                  noteId: null,
                },
                prior || null,
              ),
            )
          }
        >
          Save decision version
        </button>
        {records.map((r) => (
          <div key={r.id} className="border-t pt-2">
            <button
              className="text-left underline"
              onClick={() => onOpen(reference(r))}
            >
              {r.title}
            </button>
            <p className={muted}>
              {r.body.state} ·{" "}
              {r.supersedes
                ? "Supersedes an earlier decision"
                : "Original decision"}
            </p>
            <p className="text-sm">{r.body.rationale}</p>
          </div>
        ))}
      </section>
      <Fold title="Link a result to its input">
        <p className={muted}>
          Choose two sources from the conversation. The second depends on the
          first.
        </p>
        <button
          className={button}
          onClick={() =>
            void refresh().catch((e) => onError(workbenchErrorMessage(e)))
          }
        >
          Refresh sources and impact
        </button>
        {[
          ["Input", from, setFrom],
          ["Dependent", to, setTo],
        ].map(([label, value, set]) => (
          <select
            key={String(label)}
            aria-label={`Dependency ${label}`}
            className={input}
            value={String(value)}
            onChange={(e) => (set as (s: string) => void)(e.target.value)}
          >
            <option value="">Choose {String(label).toLowerCase()}</option>
            {!sources.some((s) => sourceKey(s) === value) && value !== "" && (
              <option value={String(value)}>
                No longer in this conversation
              </option>
            )}
            {sources.map((s) => (
              <option key={sourceKey(s)} value={sourceKey(s)}>
                {s.object.kind} · {s.object.id.slice(-10)} ·{" "}
                {s.object.revision.slice(0, 10)}
              </option>
            ))}
          </select>
        ))}
        <input
          aria-label="Dependency reason"
          className={input}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          placeholder="Why does this result depend on that input?"
        />
        <button
          className={button}
          disabled={
            busy || !fromSource || !toSource || from === to || !reason.trim()
          }
          onClick={() =>
            void run(() =>
              deskClient.relation(workspaceId, {
                input: fromSource!.object,
                dependent: toSource!.object,
                origin: "user",
                accepted: true,
                reason,
              }),
            )
          }
        >
          Save link
        </button>
      </Fold>
      <section className={card}>
        <h2 className="font-semibold">Change impact</h2>
        {impact?.impacts.map((i, n) => (
          <article key={n} className="rounded border p-3">
            <button className="underline" onClick={() => onOpen(i.object)}>
              {i.object.kind} · {i.object.id.slice(-10)}
            </button>
            <p className="text-sm">
              {i.status.replaceAll("_", " ")}: {i.reason}
            </p>
            <p className={muted}>
              {i.path.length} references in this explanation
            </p>
          </article>
        ))}
        {!impact?.impacts.length && (
          <p className={muted}>No changed inputs found among known links.</p>
        )}
        <p className={muted}>
          {impact?.complete ? "Scan complete." : "Impact scan incomplete."}{" "}
          {impact?.limitations.join(" ")}
        </p>
      </section>
      <Fold title="End this session">
        <button
          className={button}
          disabled={busy || !sessionId}
          onClick={() =>
            void run(async () =>
              setHandoff(await deskClient.handoff(sessionId!)),
            )
          }
        >
          Draft a summary of saved work
        </button>
        {handoff && (
          <>
            <p className={muted}>
              Review and accept each proposed decision before adding it to the
              project.
            </p>
            {proposals.map((n) => (
              <div className="rounded border p-3" key={n.id}>
                <p className="text-sm">{n.body}</p>
                <button
                  className={button}
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      await researchClient.updateNote({
                        noteId: n.id,
                        expectedRevision: n.revision,
                        state: "accepted",
                        operationId: `accept-${crypto.randomUUID()}`,
                      });
                      setHandoff(await deskClient.handoff(sessionId!));
                    })
                  }
                >
                  Accept this note
                </button>
              </div>
            ))}
            <details>
              <summary>Unresolved work and outputs</summary>
              <pre className="overflow-auto whitespace-pre-wrap text-xs">
                {JSON.stringify(
                  {
                    unresolved: handoff.body.unresolved,
                    outputs: handoff.body.outputs,
                  },
                  null,
                  2,
                )}
              </pre>
            </details>
          </>
        )}
        <details>
          <summary>Preview sources for a new conversation</summary>
          <pre className="whitespace-pre-wrap text-xs">{context}</pre>
        </details>
      </Fold>
    </div>
  );
}
