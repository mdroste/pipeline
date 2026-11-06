import { useEffect, useId, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { taskClient } from "../../lib/taskClient";
import { type SessionChoice } from "../../lib/taskTypes";
import {
  discoveryClient,
  discoveryError,
  newDiscovery,
  type DiscoveryMode,
  type DiscoveryDefinition,
  type DiscoveryRun,
} from "../../lib/discoveryClient";
import { Icon } from "../../ui/icons";
import BudgetPreview from "./BudgetPreview";
import SetupOptions, { CountField } from "./SetupOptions";
import ReportIssueLink from "../ReportIssueLink";

export default function Builder({
  initialMode,
  initialSessionId,
  onStarted,
}: {
  initialMode: DiscoveryMode;
  initialSessionId?: string | null;
  onStarted: (run: DiscoveryRun) => void;
}) {
  const [definition, setDefinition] = useState(() => ({
    ...newDiscovery(initialMode),
    acquireLiterature: !initialSessionId,
  }));
  const [session, setSession] = useState(initialSessionId ?? "");
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [operation, setOperation] = useState(() => crypto.randomUUID());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const mounted = useRef(true);
  const blockedId = useId();
  const modeGroup = useId();
  useEffect(() => {
    mounted.current = true;
    let live = true;
    void Promise.all([
      taskClient.sessions(),
      invoke<{ id: string; name: string }[]>("list_profiles"),
    ])
      .then(([s, p]) => {
        if (live) {
          setSessions(s.filter((s) => s.workspaceName));
          setProfiles(p);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
      mounted.current = false;
    };
  }, []);
  function patch(update: Partial<DiscoveryDefinition>) {
    setDefinition((d) => ({ ...d, ...update }));
    setOperation(crypto.randomUUID());
  }
  const connection = sessions.find(
    (choice) => choice.id === session,
  )?.connectionId;
  const invalid =
    discoveryError(definition) ??
    ((definition.ideaAgents ?? 0) > 0 &&
    connection &&
    connection !== "claude" &&
    connection !== "codex"
      ? "Choose a ChatGPT or Claude Code project for idea subagents, or set Idea subagents to 0."
      : null);

  async function start() {
    if (invalid) {
      setError(invalid);
      return;
    }
    setBusy(true);
    setError("");
    try {
      const result = await discoveryClient.start(
        {
          ...definition,
          inputPaths: definition.inputPaths
            .map((p) => p.trim())
            .filter(Boolean),
        },
        session || null,
        operation,
      );
      if (mounted.current) onStarted(result);
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <form
      className="discovery-builder"
      aria-label="New research discovery automation"
      onInvalidCapture={(e) => {
        // Reveal an invalid advanced field even if its section was closed after editing.
        (e.target as HTMLInputElement)
          .closest("details")
          ?.setAttribute("open", "");
      }}
      onSubmit={(e) => {
        e.preventDefault();
        void start();
      }}
    >
      <fieldset disabled={busy} className="ds-layout">
        <div className="ds-brief">
          <label className="ds-field ds-prompt">
            Research topics and questions
            <textarea
              required
              maxLength={16000}
              rows={3}
              value={definition.prompt}
              placeholder="What would you like to investigate? Include any sources, methods, or constraints you have in mind."
              onChange={(e) => patch({ prompt: e.target.value })}
            />
          </label>
          <label className="ds-field">
            Project context
            <select
              value={session}
              onChange={(e) => {
                setSession(e.target.value);
                patch({ acquireLiterature: !e.target.value, inputPaths: [] });
              }}
            >
              <option value="">Create a project for this research</option>
              {sessions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title} · {s.workspaceName}
                </option>
              ))}
            </select>
          </label>
          <fieldset className="ds-selection">
            <legend>Who chooses the projects?</legend>
            <div className="ds-mode-grid">
              {(
                [
                  [
                    "supervised",
                    "I’ll choose",
                    "Pause at the shortlist for my selection.",
                  ],
                  [
                    "unsupervised",
                    "Let Pipeline choose",
                    "Continue from shortlist to papers automatically.",
                  ],
                ] as const
              ).map(([mode, title, note]) => (
                <label key={mode} className="ds-mode">
                  <input
                    type="radio"
                    name={modeGroup}
                    value={mode}
                    checked={definition.mode === mode}
                    onChange={() => patch({ mode })}
                  />
                  <span>
                    <strong>{title}</strong>
                    <small>{note}</small>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
          <SetupOptions
            definition={definition}
            patch={patch}
            hasProject={Boolean(session)}
            profiles={profiles}
          />
        </div>
        <section className="ds-plan" aria-label="Portfolio plan">
          <header>
            <Icon name="grid" />
            <h3>Portfolio plan</h3>
          </header>
          <div className="ds-targets">
            <CountField
              label="Candidate projects"
              hint="Explore 50–100"
              min={50}
              max={100}
              value={definition.candidateCount}
              onChange={(candidateCount) => patch({ candidateCount })}
            />
            <CountField
              label="Shortlisted projects"
              hint="Keep 1–25 proposals"
              min={1}
              max={25}
              value={definition.shortlistCount}
              onChange={(shortlistCount) => patch({ shortlistCount })}
            />
            <CountField
              label="Papers requested"
              hint="Develop up to 9"
              min={1}
              max={9}
              value={definition.paperCount}
              onChange={(paperCount) => patch({ paperCount })}
            />
          </div>
          <p className="ds-plan-note">
            The final report ranks the drafts and lists unfinished work.
          </p>
          <BudgetPreview definition={definition} />
          {error && (
            <p className="task-error" role="alert">
              {error}
              <ReportIssueLink />
            </p>
          )}
          <button
            type="submit"
            className="task-primary ds-start"
            disabled={busy || Boolean(invalid)}
            aria-describedby={invalid ? blockedId : undefined}
          >
            {busy ? "Starting…" : "Start research discovery"}
            <Icon name="arrow" />
          </button>
          {invalid && (
            <p id={blockedId} className="ds-blocked">
              {invalid}
            </p>
          )}
          <p className="ds-runtime">
            Keep Pipeline open and your computer awake during the run.
          </p>
        </section>
      </fieldset>
    </form>
  );
}
