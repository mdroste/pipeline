import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Monitor, MonitorState } from "../../lib/programClient";
import {
  button,
  input,
  card,
  muted,
  Field,
  Inspect,
  operation,
  useProgram,
  type DeskProps,
} from "./shared";
export default function Checks(props: DeskProps & { active?: boolean }) {
  const p = useProgram(props, ["dataset"]);
  const [title, setTitle] = useState("Tracked research change"),
    [monitor, setMonitor] = useState<Monitor>({
      kind: "accepted_file",
      target: "",
      intervalSeconds: 3600,
      notify: true,
      networkConsent: false,
    }),
    [state, setState] = useState<MonitorState | null>(null);
  const refresh = () =>
    p.call<MonitorState>({ action: "monitors" }).then(setState);
  useEffect(() => {
    if (props.active === false) return;
    let alive = true;
    const load = () => {
      void p
        .call<MonitorState>({ action: "monitors" })
        .then((v) => {
          if (alive) setState(v);
        })
        .catch(() => undefined);
    };
    load();
    const timer = setInterval(load, 15000);
    const event = listen<{ workspaceId: string }>(
      "workbench-research-attention",
      (e) => {
        if (e.payload.workspaceId === props.workspaceId) load();
      },
    );
    return () => {
      alive = false;
      clearInterval(timer);
      void event.then((unlisten) => unlisten());
    };
  }, [props.workspaceId, props.active]);
  return (
    <div className="space-y-5">
      <section className={card}>
        <h2 className="font-semibold">Scheduled research checks</h2>
        <p className={muted}>
          Check project records or refresh a literature search while Pipeline is open.
          The first check sets a baseline; later checks report changes.
          Missed checks are combined into one.
        </p>
        <Field label="Check title">
          <input
            className={input}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </Field>
        <Field label="What to check">
          <select
            className={input}
            value={monitor.kind}
            onChange={(e) =>
              setMonitor((m) => ({
                ...m,
                kind: e.target.value,
                target: "",
                networkConsent: false,
              }))
            }
          >
            <option value="accepted_file">Accepted file changed</option>
            <option value="dataset">Tracked dataset version changed</option>
            <option value="results">New local structured results</option>
            <option value="execution">Selected execution status</option>
            <option value="metadata_query">
              Saved Crossref metadata query
            </option>
          </select>
        </Field>
        {monitor.kind === "dataset" ? (
          <Field label="Dataset to track">
            <select
              className={input}
              value={monitor.target}
              onChange={(e) =>
                setMonitor((m) => ({ ...m, target: e.target.value }))
              }
            >
              <option value="">Choose dataset</option>
              {p.records.dataset?.map((d) => (
                <option value={d.id} key={d.id}>
                  {d.title}
                </option>
              ))}
            </select>
          </Field>
        ) : monitor.kind === "execution" ? (
          <Field label="Execution to track">
            <select
              className={input}
              value={monitor.target}
              onChange={(e) =>
                setMonitor((m) => ({ ...m, target: e.target.value }))
              }
            >
              <option value="">Choose execution</option>
              {p.choices?.executions.map((e) => (
                <option value={e.id} key={e.id}>
                  {e.outcome} · {e.adapter} ·{" "}
                  {new Date(e.createdAt).toLocaleString()}
                </option>
              ))}
            </select>
          </Field>
        ) : (
          monitor.kind !== "results" && (
            <Field
              label={
                monitor.kind === "metadata_query"
                  ? "Query sent to Crossref"
                  : "Relative accepted file path"
              }
            >
              <input
                className={input}
                value={monitor.target}
                onChange={(e) =>
                  setMonitor((m) => ({ ...m, target: e.target.value }))
                }
                placeholder={
                  monitor.kind === "accepted_file"
                    ? "paper.tex"
                    : "Bibliographic query or DOI"
                }
              />
            </Field>
          )
        )}
        <Field label="Check interval (minutes)">
          <input
            className={input}
            type="number"
            min="1"
            max="43200"
            value={monitor.intervalSeconds / 60}
            onChange={(e) =>
              setMonitor((m) => ({
                ...m,
                intervalSeconds: Number(e.target.value) * 60,
              }))
            }
          />
        </Field>
        <label className="block text-xs">
          <input
            type="checkbox"
            checked={monitor.notify}
            onChange={(e) =>
              setMonitor((m) => ({ ...m, notify: e.target.checked }))
            }
          />{" "}
          Show an in-app notification for meaningful changes
        </label>
        {monitor.kind === "metadata_query" && (
          <label className="block text-xs">
            <input
              type="checkbox"
              checked={monitor.networkConsent}
              onChange={(e) =>
                setMonitor((m) => ({ ...m, networkConsent: e.target.checked }))
              }
            />{" "}
            Allow this query to be sent to api.crossref.org at each check.
            Project acquisition access must also be enabled.
          </label>
        )}
        <button
          className={button}
          disabled={
            p.busy ||
            !title ||
            (monitor.kind !== "results" && !monitor.target) ||
            (monitor.kind === "metadata_query" && !monitor.networkConsent)
          }
          onClick={() =>
            void p.run(async () => {
              await p.call({
                action: "saveMonitor",
                title,
                monitor,
                operationId: operation(),
              });
              await refresh();
            })
          }
        >
          Save and enable check
        </button>
      </section>
      {state && <p className={muted}>{state.lifecycle}</p>}
      {state?.attention
        .filter((a) => !a.acknowledgedAt)
        .map((a) => (
          <section className={`${card} border-amber-300`} key={a.id}>
            <h3>{a.body.title}</h3>
            <Inspect value={a.body.outcome} label="What changed" />
            <button
              className={button}
              onClick={() =>
                void p.run(async () =>
                  setState(
                    await p.call({
                      action: "monitorControl",
                      id: a.id,
                      revision: 0,
                      control: "acknowledge",
                    }),
                  ),
                )
              }
            >
              Acknowledge this attention item
            </button>
          </section>
        ))}
      {state?.checks.map((c) => (
        <section className={card} key={c.id}>
          <h3 className="font-semibold">
            {c.title} · {c.enabled ? "enabled" : "paused"}
          </h3>
          <p className={muted}>
            Next due: {new Date(c.nextDueAt * 1000).toLocaleString()} · last
            checked:{" "}
            {c.lastCheckedAt
              ? new Date(c.lastCheckedAt * 1000).toLocaleString()
              : "baseline pending"}
          </p>
          <Inspect value={{ scope: c.spec, lastOutcome: c.lastOutcome }} />
          <button
            className={button}
            disabled={p.busy}
            onClick={() =>
              void p.run(async () =>
                setState(
                  await p.call({
                    action: "monitorControl",
                    id: c.id,
                    revision: c.revision,
                    control: c.enabled ? "pause" : "resume",
                  }),
                ),
              )
            }
          >
            {c.enabled ? "Pause" : "Resume"} check
          </button>
        </section>
      ))}
    </div>
  );
}
