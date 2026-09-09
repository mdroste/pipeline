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
import { Outline } from "./shared";
import { Timing } from "./Timing";

export default function Builder({
  initialSessionId,
  onPrepared,
  onClose,
}: {
  initialSessionId?: string | null;
  onPrepared: (run: TaskRun) => void;
  onClose: () => void;
}) {
  const [kind, setKind] = useState<"review" | "prompt" | "input">("review");
  const [prompt, setPrompt] = useState("");
  const [profile, setProfile] = useState("");
  const [rounds, setRounds] = useState(3);
  const [path, setPath] = useState("");
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [session, setSession] = useState(initialSessionId ?? "");
  const [trigger, setTrigger] = useState<Trigger>({ kind: "now" });
  const [saved, setSaved] = useState<{ id: string; chain: Chain }[]>([]);
  const [advanced, setAdvanced] = useState<string | null>(null);
  const [inputs, setInputs] = useState("{}");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const operation = useRef(crypto.randomUUID());
  const generated = template(kind, prompt, profile, rounds, path);
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
      const chain = await taskClient.validate(
        advanced ?? JSON.stringify(generated),
      );
      const parsed: unknown = JSON.parse(inputs);
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed))
        throw new Error("Inputs must be a JSON object");
      const run = await taskClient.prepare(
        chain,
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
  return (
    <section className="task-builder" aria-label="New automation">
      <div className="task-section-heading">
        <div>
          <h2>New automation</h2>
          <p className="task-muted">
            Review and revise a paper, schedule a follow-up, or wait for input.
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
              "Review a paper and revise it up to a set limit.",
            ],
            ["prompt", "Follow up", "Continue a conversation now or later."],
            [
              "input",
              "Wait for input",
              "Pick up when the missing input arrives.",
            ],
          ] as const
        ).map(([id, title, subtitle]) => (
          <button
            key={id}
            type="button"
            aria-label={title}
            aria-pressed={kind === id && advanced === null}
            onClick={() => {
              setKind(id);
              setAdvanced(null);
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
      {advanced === null ? (
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
                  Review profile
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
                    onChange={(e) => setRounds(Number(e.target.value))}
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
          <Outline steps={generated.steps} />
        </>
      ) : (
        <>
          <label>
            Automation definition
            <textarea
              className="task-code"
              spellCheck={false}
              rows={19}
              value={advanced}
              onChange={(e) => setAdvanced(e.target.value)}
            />
          </label>
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
          <p className="task-muted">
            Supports conversation, snapshot, review, check, delivery, delay,
            input, if, repeat, parallel, forEach, and embedded automation steps.
            Bind outputs with a step ID and JSON pointer.
          </p>
        </>
      )}
      <div className="task-actions">
        <button
          type="button"
          onClick={() =>
            setAdvanced(
              advanced === null ? JSON.stringify(generated, null, 2) : null,
            )
          }
        >
          {advanced === null ? "Edit definition" : "Use template"}
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
                .then((c) => setAdvanced(JSON.stringify(c, null, 2)))
                .catch((e) => setError(String(e)));
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
              if (c) setAdvanced(JSON.stringify(c.chain, null, 2));
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
      </div>
      <Timing value={trigger} onChange={setTrigger} />
      {error && (
        <p className="task-error" role="alert">
          {error}
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
            (advanced === null &&
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
