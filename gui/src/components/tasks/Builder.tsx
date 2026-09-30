import { useEffect, useRef, useState } from "react";
import { appClient } from "../../lib/appClient";
import {
  taskClient,
  template,
  type Chain,
  type Json,
  type SessionChoice,
  type TaskRun,
  type Trigger,
} from "../../lib/taskClient";
import OutlineEditor from "./OutlineEditor";
import { Timing } from "./Timing";

const LIMITS = { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 };

type TemplateKind = "review" | "prompt" | "input" | "blank";

export default function Builder({
  initialSessionId,
  initialPath,
  onPrepared,
  onClose,
}: {
  initialSessionId?: string | null;
  /** Prefill the review-and-revise template with this input file. */
  initialPath?: string | null;
  onPrepared: (run: TaskRun) => void;
  onClose: () => void;
}) {
  const [kind, setKind] = useState<TemplateKind>("review");
  const [prompt, setPrompt] = useState("");
  const [profile, setProfile] = useState("");
  const [rounds, setRounds] = useState(3);
  const [path, setPath] = useState(initialPath ?? "");
  // While pristine, the outline regenerates from the quick fields above it;
  // the first structural edit takes ownership of the chain.
  const [pristine, setPristine] = useState(true);
  const [editedChain, setEditedChain] = useState<Chain | null>(null);
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [session, setSession] = useState(initialSessionId ?? "");
  const [trigger, setTrigger] = useState<Trigger>({ kind: "now" });
  const [saved, setSaved] = useState<{ id: string; chain: Chain }[]>([]);
  const [inputs, setInputs] = useState("{}");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const operation = useRef(crypto.randomUUID());
  const blank: Chain = {
    schemaVersion: 1,
    name: "New automation",
    description: "",
    steps: [],
    limits: LIMITS,
  };
  const chain =
    editedChain ??
    (kind === "blank" ? blank : template(kind, prompt, profile, rounds, path));
  const setChain = (next: Chain) => {
    setEditedChain(next);
    setPristine(false);
  };
  useEffect(() => {
    let alive = true;
    void Promise.all([
      appClient.profiles(),
      taskClient.sessions(),
      taskClient.chains(),
    ])
      .then(([p, s, c]) => {
        if (!alive) return;
        setProfiles(p);
        setProfile(
          p.find((v) => /automatic.*paper|auto.*review/i.test(v.name))?.id ??
            p[0]?.id ??
            "",
        );
        setSessions(s);
        setSaved(c);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);
  async function prepare() {
    setBusy(true);
    setError("");
    try {
      const validated = await taskClient.validate(JSON.stringify(chain));
      const parsed: unknown = JSON.parse(inputs);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
        throw new Error("Inputs must be a JSON object");
      const run = await taskClient.prepare(
        validated,
        session || null,
        parsed as Record<string, Json>,
        trigger,
        operation.current,
      );
      onPrepared(run);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function saveAsTemplate() {
    setError("");
    setNotice("");
    try {
      const validated = await taskClient.validate(JSON.stringify(chain));
      await taskClient.saveChain(validated);
      setSaved(await taskClient.chains());
      setNotice("Saved. It is available under Saved automations.");
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <section className="task-builder" aria-label="New automation">
      <div className="task-section-heading">
        <div>
          <h2>New automation</h2>
          <p className="task-muted">
            Chain assistant work, reviews, waits, and decisions. Start from a
            template or compose the steps yourself.
          </p>
        </div>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close new automation"
        >
          ×
        </button>
      </div>
      <div className="task-template-picker" aria-label="Automation template">
        {(
          [
            [
              "review",
              "Review and revise",
              "Draft, review, and revise up to a set limit.",
            ],
            ["prompt", "Follow up", "Continue a conversation now or later."],
            [
              "input",
              "Wait for input",
              "Pick up when the missing input arrives.",
            ],
            ["blank", "Start empty", "Compose the steps yourself."],
          ] as const
        ).map(([id, title, subtitle]) => (
          <button
            key={id}
            type="button"
            aria-label={title}
            aria-pressed={kind === id && editedChain === null}
            onClick={() => {
              setKind(id);
              setEditedChain(null);
              setPristine(true);
            }}
          >
            <strong>{title}</strong>
            <span>{subtitle}</span>
          </button>
        ))}
      </div>
      <label>
        Conversation
        <select value={session} onChange={(e) => setSession(e.target.value)}>
          <option value="">Select a conversation</option>
          {sessions.map((s) => (
            <option key={s.id} value={s.id}>
              {s.title}
              {s.workspaceName ? ` · ${s.workspaceName}` : ""}
            </option>
          ))}
        </select>
      </label>
      {pristine && kind !== "blank" && (
        <>
          <label>
            {kind === "review"
              ? "Idea or instructions"
              : "What should happen next?"}
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={4}
              placeholder={
                kind === "review"
                  ? "Develop a paper about…"
                  : "Review the new results and explain what changed…"
              }
            />
          </label>
          {kind === "review" && (
            <>
              <div className="task-form-row">
                <label>
                  Review workflow
                  <select
                    value={profile}
                    onChange={(e) => setProfile(e.target.value)}
                  >
                    {profiles.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Maximum reviews
                  <input
                    type="number"
                    min={1}
                    max={32}
                    value={rounds}
                    onChange={(e) =>
                      setRounds(
                        Math.min(32, Math.max(1, Number(e.target.value) || 1)),
                      )
                    }
                  />
                </label>
              </div>
              <details>
                <summary>Start from an existing paper</summary>
                <label>
                  File within the conversation’s folder
                  <input
                    value={path}
                    onChange={(e) => setPath(e.target.value)}
                    placeholder="/path/to/paper.pdf"
                  />
                </label>
                <p className="task-muted">
                  Leave blank to draft from your idea. Revisions from this
                  template are saved as Markdown.
                </p>
              </details>
            </>
          )}
        </>
      )}
      {!pristine && (
        <label>
          Automation name
          <input
            value={chain.name}
            onChange={(e) => setChain({ ...chain, name: e.target.value })}
          />
        </label>
      )}
      <OutlineEditor chain={chain} onChange={setChain} profiles={profiles} />
      <details className="task-advanced">
        <summary>Definition (advanced)</summary>
        <pre className="task-code" aria-label="Automation definition">
          {JSON.stringify(chain, null, 2)}
        </pre>
        <div className="task-actions">
          <button
            type="button"
            onClick={() =>
              void navigator.clipboard
                ?.writeText(JSON.stringify(chain, null, 2))
                .then(() => setNotice("Definition copied."))
                .catch(() => setError("Could not copy the definition."))
            }
          >
            Copy definition
          </button>
          <label className="task-import">
            Import automation
            <input
              type="file"
              accept=".json,application/json"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                if (file.size > 262144) {
                  setError("Chain exceeds 256 KiB");
                  return;
                }
                void file
                  .text()
                  .then(taskClient.validate)
                  .then((c) => setChain(c))
                  .catch((err) => setError(String(err)));
                e.target.value = "";
              }}
            />
          </label>
          {saved.length > 0 && (
            <select
              aria-label="Use a saved automation"
              value=""
              onChange={(e) => {
                const c = saved.find((v) => v.id === e.target.value);
                if (c) setChain(structuredClone(c.chain));
              }}
            >
              <option value="">Saved automations…</option>
              {saved.map((c) => (
                <option value={c.id} key={c.id}>
                  {c.chain.name}
                </option>
              ))}
            </select>
          )}
          <button type="button" onClick={() => void saveAsTemplate()}>
            Save as template
          </button>
        </div>
        <label>
          Named inputs
          <textarea
            className="task-code"
            spellCheck={false}
            rows={3}
            value={inputs}
            onChange={(e) => setInputs(e.target.value)}
          />
        </label>
        <div className="task-form-row">
          <label>
            Action limit
            <input
              type="number"
              min={1}
              max={64}
              value={chain.limits.maxActions}
              onChange={(e) =>
                setChain({
                  ...chain,
                  limits: {
                    ...chain.limits,
                    maxActions: Math.min(
                      64,
                      Math.max(1, Number(e.target.value)),
                    ),
                  },
                })
              }
            />
          </label>
          <label>
            Deadline (hours)
            <input
              type="number"
              min={1}
              value={chain.limits.deadlineHours}
              onChange={(e) =>
                setChain({
                  ...chain,
                  limits: {
                    ...chain.limits,
                    deadlineHours: Math.max(1, Number(e.target.value)),
                  },
                })
              }
            />
          </label>
        </div>
      </details>
      <Timing value={trigger} onChange={setTrigger} />
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="task-muted" role="status">
          {notice}
        </p>
      )}
      <div className="task-footer">
        <p className="task-muted">
          You’ll review the exact scope before starting.
        </p>
        <button
          className="task-primary"
          disabled={
            busy ||
            chain.steps.length === 0 ||
            (pristine &&
              kind !== "blank" &&
              (!session ||
                (kind === "review" && !profile) ||
                (!prompt.trim() && !path)))
          }
          onClick={() => void prepare()}
        >
          {busy ? "Preparing…" : "Prepare automation →"}
        </button>
      </div>
    </section>
  );
}
