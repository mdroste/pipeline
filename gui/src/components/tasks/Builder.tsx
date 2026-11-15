import { AllowanceField } from "./Budget";
import { workflowProfileClient } from "../../lib/workflowProfileClient";
/** Compact setup separates instructions and timing from the complete step editor. */
import { useEffect, useRef, useState } from "react";
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
import useTabList from "../../hooks/useTabList";
import "./Builder.css";

const LIMITS = { maxActions: 64, deadlineHours: 168, actionTimeoutSecs: 7200 };

export type TemplateKind = "review" | "prompt" | "input" | "blank";
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
  initialKind = "review",
  onChooseType,
  initialPath,
  onPrepared,
  onDirtyChange,
  onClose,
}: {
  initialSessionId?: string | null;
  initialKind?: TemplateKind;
  onChooseType?: () => void;
  /** Prefill the review-and-revise template with this input file. */
  initialPath?: string | null;
  onPrepared: (run: TaskRun) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onClose: () => void;
}) {
  useEffect(() => () => onDirtyChange?.(false), [onDirtyChange]);
  const [kind, setKind] = useState<TemplateKind>(initialKind);
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
  const [name, setName] = useState(
    initialKind === "blank"
      ? "New automation"
      : template(initialKind, "", "").name,
  );
  const [section, setSection] = useState(
    initialKind === "blank" ? "steps" : "setup",
  );
  const tabs = useTabList(["setup", "steps", "advanced"], section, setSection);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    heading.current?.focus({ preventScroll: true });
    heading.current?.scrollIntoView?.({ block: "nearest" });
  }, []);
  const [selectedTemplate, setSelectedTemplate] = useState<string>(initialKind);
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
    setSection(draft.kind === "blank" || !draft.pristine ? "steps" : "setup");
    setError("");
    setNotice("Your edits stay with each template while this builder is open.");
    onDirtyChange?.(true);
  }
  useEffect(() => {
    let alive = true;
    void Promise.all([
      workflowProfileClient.profiles(),
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
        <h2 ref={heading} tabIndex={-1}>
          New automation
        </h2>
        <div className="automation-heading-actions">
          {onChooseType && (
            <button
              type="button"
              className="ui-button ui-button--sm"
              onClick={onChooseType}
            >
              Change type
            </button>
          )}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close new automation"
          >
            ×
          </button>
        </div>
      </div>
      <div className="task-template-picker" aria-label="Automation template">
        {(
          [
            ["review", "Review and revise"],
            ["prompt", "Follow up"],
            ["input", "Wait for input"],
            ["blank", "Start empty"],
          ] as const
        ).map(([id, title]) => (
          <button
            key={id}
            type="button"
            aria-label={title}
            aria-pressed={selectedTemplate === id}
            onClick={() => selectTemplate(id)}
          >
            <strong>{title}</strong>
          </button>
        ))}
      </div>
      <fieldset disabled={busy} className="automation-builder-fields">
        <div className="automation-builder-context">
          <label>
            Automation name
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
          <label>
            Conversation
            <select
              value={session}
              onChange={(e) => setSession(e.target.value)}
            >
              <option value="">Select a conversation</option>
              {sessions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title}
                  {s.workspaceName ? ` · ${s.workspaceName}` : ""}
                </option>
              ))}
            </select>
          </label>
        </div>
        <div
          className="ui-page-tabs"
          role="tablist"
          aria-label="Automation settings"
        >
          {["setup", "steps", "advanced"].map((id) => (
            <button key={id} {...tabs.tabProps(id)}>
              {
                {
                  setup: "Setup & schedule",
                  steps: "Steps & limits",
                  advanced: "Definition (advanced)",
                }[id]
              }
            </button>
          ))}
        </div>
        <div {...tabs.panelProps("setup")} hidden={section !== "setup"}>
          <div className="automation-builder-columns">
            <section
              className="automation-builder-card automation-builder-stack"
              aria-label="Instructions"
            >
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
                          Leave blank to draft from your idea. Revisions from
                          this template are saved as Markdown.
                        </p>
                      </details>
                    </>
                  )}
                </>
              )}
              {(!pristine || kind === "blank") && (
                <div>
                  <h3>Custom steps</h3>
                  <p className="task-muted">
                    Edit the steps and limits to define what this automation
                    does.
                  </p>
                  <button type="button" onClick={() => setSection("steps")}>
                    Edit steps and limits
                  </button>
                </div>
              )}
            </section>
            <section className="automation-builder-card" aria-label="Schedule">
              <Timing
                value={trigger}
                onChange={(next) => {
                  onDirtyChange?.(true);
                  setTrigger(next);
                }}
              />
            </section>
          </div>
        </div>
        <div {...tabs.panelProps("steps")} hidden={section !== "steps"}>
          <div className="automation-builder-columns automation-builder-step-columns">
            <section
              className="automation-builder-card"
              aria-label="Automation steps"
            >
              <h3>Steps</h3>
              <OutlineEditor
                chain={chain}
                onChange={setChain}
                profiles={profiles}
              />
            </section>
            <section
              className="automation-builder-card automation-builder-stack"
              aria-label="Limits and templates"
            >
              <h3>Limits</h3>
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
                <AllowanceField
                  cents={chain.limits.maxApiCostCents}
                  onChange={(maxApiCostCents) =>
                    setChain({
                      ...chain,
                      limits: { ...chain.limits, maxApiCostCents },
                    })
                  }
                />
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
              <h3>Templates</h3>
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
                    {saved.length
                      ? "Choose a template"
                      : "No saved templates yet"}
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
            </section>
          </div>
        </div>
        <div {...tabs.panelProps("advanced")} hidden={section !== "advanced"}>
          <section className="automation-builder-card task-advanced">
            <h3>Definition (advanced)</h3>
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
          </section>
        </div>
      </fieldset>
      <footer className="automation-builder-footer">
        <div>
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
          <p className="task-muted">
            You’ll review the exact scope before starting.
          </p>
        </div>
        <button
          className="ui-button ui-button--primary"
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
      </footer>
    </section>
  );
}
