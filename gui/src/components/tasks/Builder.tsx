import { useEffect, useRef, useState } from "react";
import { appClient } from "../../lib/appClient";
import { taskClient } from "../../lib/taskClient";
import { template } from "../../lib/taskTemplates";
import {
  type Chain,
  type Json,
  type SessionChoice,
  type TaskRun,
  type Trigger,
} from "../../lib/taskTypes";
import { NumberField } from "./NumberField";
import OutlineEditor from "./OutlineEditor";
import { Timing } from "./Timing";
import ReportIssueLink from "../ReportIssueLink";
import PaperInput from "./PaperInput";

const LIMITS = { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 };

type TemplateKind = "review" | "prompt" | "input" | "blank";
type Draft = {
  kind: TemplateKind;
  prompt: string;
  profile: string;
  rounds: number;
  path: string;
  pristine: boolean;
  editedChain: Chain | null;
  name: string;
  inputs: string;
  trigger: Trigger;
};

export default function Builder({
  initialSessionId,
  initialPath,
  onPrepared,
  onDirtyChange,
  onClose,
}: {
  initialSessionId?: string | null;
  /** Prefill the review-and-revise template with this input file. */
  initialPath?: string | null;
  onPrepared: (run: TaskRun) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onClose: () => void;
}) {
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);
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
  const [name, setName] = useState("Review and revise");
  const [selectedTemplate, setSelectedTemplate] = useState("review");
  const drafts = useRef(new Map<string, Draft>());
  const [pathValid, setPathValid] = useState(!initialPath);
  const operation = useRef(crypto.randomUUID());
  const blank: Chain = {
    schemaVersion: 1,
    name: "New automation",
    description: "",
    steps: [],
    limits: LIMITS,
  };
  const source =
    editedChain ??
    (kind === "blank" ? blank : template(kind, prompt, profile, rounds, path));
  const chain = { ...source, name };
  const setChain = (next: Chain) => {
    onDirtyChange?.(true);
    setEditedChain(next);
    setPristine(false);
    setName(next.name);
  };
  function selectTemplate(key: string, savedChain?: Chain) {
    if (key === selectedTemplate) return;
    drafts.current.set(
      selectedTemplate,
      structuredClone({
        kind,
        prompt,
        profile,
        rounds,
        path,
        pristine,
        editedChain,
        name,
        inputs,
        trigger,
      }),
    );
    const nextKind = savedChain ? "blank" : (key as TemplateKind);
    const draft = drafts.current.get(key) ?? {
      kind: nextKind,
      prompt: "",
      profile,
      rounds: 3,
      path: "",
      pristine: !savedChain,
      editedChain: savedChain ? structuredClone(savedChain) : null,
      name:
        savedChain?.name ??
        (nextKind === "blank"
          ? "New automation"
          : template(nextKind, "", profile).name),
      inputs: "{}",
      trigger: { kind: "now" } as Trigger,
    };
    setKind(draft.kind);
    setPrompt(draft.prompt);
    setProfile(draft.profile);
    setRounds(draft.rounds);
    setPath(draft.path);
    setPristine(draft.pristine);
    setEditedChain(draft.editedChain);
    setName(draft.name);
    setInputs(draft.inputs);
    setTrigger(draft.trigger);
    setSelectedTemplate(key);
    setError("");
    setNotice("Your edits stay with each template while this builder is open.");
    onDirtyChange?.(true);
  }
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
      onDirtyChange?.(false);
      onPrepared(run);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function saveAsTemplate() {
    if (busy || !name.trim()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const validated = await taskClient.validate(JSON.stringify(chain));
      await taskClient.saveChain(validated);
      setSaved(await taskClient.chains());
      setNotice(`Saved “${chain.name}” to My templates.`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      className="task-builder"
      aria-label="New automation"
      onChangeCapture={() => onDirtyChange?.(true)}
    >
      <div className="task-section-heading">
        <div>
          <h2>New automation</h2>
          <p className="task-muted">
            Combine assistant work, reviews, and pauses for input. Start with a
            template or add your own steps.
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
            aria-pressed={selectedTemplate === id}
            onClick={() => selectTemplate(id)}
          >
            <strong>{title}</strong>
            <span>{subtitle}</span>
          </button>
        ))}
      </div>
      <div className="task-form-row">
        <label>
          Automation name
          <input value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <label>
          My templates
          <select
            value={
              selectedTemplate.startsWith("saved:")
                ? selectedTemplate.slice(6)
                : ""
            }
            onChange={(e) => {
              const item = saved.find((v) => v.id === e.target.value);
              if (item) selectTemplate(`saved:${item.id}`, item.chain);
            }}
          >
            <option value="">
              {saved.length ? "Choose a template" : "No saved templates yet"}
            </option>
            {saved.map((item) => (
              <option key={item.id} value={item.id}>
                {item.chain.name}
              </option>
            ))}
          </select>
        </label>
        <button
          type="button"
          disabled={busy || !name.trim() || !chain.steps.length}
          onClick={() => void saveAsTemplate()}
        >
          Save as template
        </button>
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
                  <NumberField
                    min={1}
                    max={32}
                    value={rounds}
                    onChange={setRounds}
                  />
                </label>
              </div>
              <details>
                <summary>Start from an existing paper</summary>
                <PaperInput
                  sessionId={session}
                  value={path}
                  onChange={(next) => {
                    onDirtyChange?.(true);
                    setPath(next);
                  }}
                  onValidChange={setPathValid}
                />
                <p className="task-muted">
                  Leave blank to draft from your idea. Revisions from this
                  template are saved as Markdown.
                </p>
              </details>
            </>
          )}
        </>
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
        {/* Same ranges as validate() in orchestration/definition.rs. */}
        <div className="task-form-row">
          <label>
            Action limit
            <NumberField
              min={1}
              max={1024}
              value={chain.limits.maxActions}
              onChange={(maxActions) =>
                setChain({
                  ...chain,
                  limits: { ...chain.limits, maxActions },
                })
              }
            />
          </label>
          <label>
            Deadline (hours)
            <NumberField
              min={1}
              max={8760}
              value={chain.limits.deadlineHours}
              onChange={(deadlineHours) =>
                setChain({
                  ...chain,
                  limits: { ...chain.limits, deadlineHours },
                })
              }
            />
          </label>
        </div>
      </details>
      <Timing
        value={trigger}
        onChange={(next) => {
          onDirtyChange?.(true);
          setTrigger(next);
        }}
      />
      {error && (
        <p className="task-error" role="alert">
          {error}
          <ReportIssueLink />
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
            !name.trim() ||
            (pristine && kind === "review" && !!path && !pathValid) ||
            chain.steps.length === 0 ||
            (pristine &&
              kind !== "blank" &&
              (!session ||
                (kind === "review" && !profile) ||
                (!prompt.trim() && !path)))
          }
          onClick={() => void prepare()}
        >
          {busy ? "Preparing…" : "Preview automation →"}
        </button>
      </div>
    </section>
  );
}
