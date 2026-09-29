import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { taskClient, type SessionChoice } from "../../lib/taskClient";
import {
  discoveryClient,
  discoveryError,
  discoveryNames,
  newDiscovery,
  type DiscoveryMode,
  type DiscoveryDefinition,
  type DiscoveryRun,
} from "../../lib/discoveryClient";

export default function Builder({
  initialMode,
  initialSessionId,
  onStarted,
  onClose,
}: {
  initialMode: DiscoveryMode;
  initialSessionId?: string | null;
  onStarted: (run: DiscoveryRun) => void;
  onClose: () => void;
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
  async function start() {
    const invalid = discoveryError(definition);
    if (invalid) {
      setError(invalid);
      return;
    }
    setBusy(true);
    setError("");
    try {
      const result = await discoveryClient.start(
        definition,
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
  const number = (
    key: keyof DiscoveryDefinition,
    label: string,
    min: number,
    max: number,
  ) => (
    <label key={key}>
      {label}
      <input
        type="number"
        min={min}
        max={max}
        required
        value={Number(definition[key])}
        onChange={(e) => patch({ [key]: Number(e.target.value) })}
      />
    </label>
  );
  return (
    <form
      className="task-detail discovery-builder"
      aria-label="New self-discovery automation"
      onSubmit={(e) => {
        e.preventDefault();
        void start();
      }}
    >
      <div className="task-section-heading">
        <h2>Discover and develop research</h2>
        <button
          type="button"
          aria-label="Close self-discovery builder"
          onClick={onClose}
        >
          ×
        </button>
      </div>
      <p className="task-muted">
        Describe a research area in any academic field. Pipeline identifies the
        relevant methods, evidence standards, and reviewers.
      </p>
      <fieldset disabled={busy} className="mission-fields">
        <label>
          Mode
          <select
            value={definition.mode}
            onChange={(e) => patch({ mode: e.target.value as DiscoveryMode })}
          >
            {Object.entries(discoveryNames).map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label>
          Research topics and questions
          <textarea
            required
            maxLength={16000}
            rows={7}
            value={definition.prompt}
            placeholder="Describe the topics, questions, sources, constraints, and kinds of contribution you have in mind."
            onChange={(e) => patch({ prompt: e.target.value })}
          />
        </label>
        <label>
          Project context
          <select
            value={session}
            onChange={(e) => {
              setSession(e.target.value);
              setOperation(crypto.randomUUID());
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
        <div className="mission-budget-grid">
          {number("candidateCount", "Candidate projects", 50, 100)}
          {number("shortlistCount", "Shortlisted projects", 1, 25)}
          {number("paperCount", "Papers requested", 1, 9)}
        </div>
        <p className="discovery-funnel">
          Explore {definition.candidateCount} projects → assess and shortlist{" "}
          {definition.shortlistCount} →{" "}
          {definition.mode === "supervised"
            ? "you choose"
            : "automatically choose"}{" "}
          {definition.paperCount} → research, write, review, and rank.
        </p>
        <details>
          <summary>Research and review settings</summary>
          <div className="mission-budget-grid">
            {number("proposalReviewers", "Detailed proposal reviewers", 0, 4)}
            {number("proposalRevisionPasses", "Proposal revision passes", 0, 2)}
            {number("paperReviewers", "Paper reviewers per round", 0, 4)}
            {number("revisionPasses", "Revision passes", 0, 5)}
            {number("researchRounds", "Investigation rounds per paper", 1, 16)}
            {number("maxReplacements", "Unsupervised replacement limit", 0, 5)}
          </div>
          <label className="mission-check">
            <input
              type="checkbox"
              checked={definition.challengeResearch}
              onChange={(e) => patch({ challengeResearch: e.target.checked })}
            />
            Challenge findings after each research round
          </label>
          <label className="mission-check">
            <input
              type="checkbox"
              checked={definition.allowComputation}
              onChange={(e) => patch({ allowComputation: e.target.checked })}
            />
            Allow code and computation in isolated paper folders
          </label>
          <p className="task-muted">
            Uses installed tools within the paper folder, with command
            networking disabled. Original project files remain outside that
            execution scope.
          </p>
          <label>
            Research input files
            <textarea
              rows={3}
              disabled={!session}
              value={definition.inputPaths.join("\n")}
              placeholder="One file path per line, relative to the selected project folder"
              onChange={(e) =>
                patch({
                  inputPaths: e.target.value
                    .split("\n")
                    .filter((p) => p.trim()),
                })
              }
            />
          </label>
          <p className="task-muted">
            These files are captured at Start and copied into each paper's
            folder. Include only data and source material you want available to
            the research models.
          </p>
          <label className="mission-check">
            <input
              type="checkbox"
              checked={definition.acquireLiterature}
              onChange={(e) => patch({ acquireLiterature: e.target.checked })}
            />
            Search public literature records and available abstracts
          </label>
          <p className="task-muted">
            Enables literature acquisition for a new project. Existing projects
            require Library acquisition to be enabled. Bibliographic metadata
            does not establish full-text access.
          </p>
          <label>
            Additional structured Review
            <select
              value={definition.reviewProfileId ?? ""}
              onChange={(e) =>
                patch({ reviewProfileId: e.target.value || null })
              }
            >
              <option value="">Built-in field-adaptive referees only</option>
              {profiles.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            Author model
            <input
              value={definition.authorModel ?? ""}
              placeholder="Current configured model"
              onChange={(e) => patch({ authorModel: e.target.value || null })}
            />
          </label>
          <label>
            Reviewer model
            <input
              value={definition.reviewerModel ?? ""}
              placeholder="Current configured model"
              onChange={(e) => patch({ reviewerModel: e.target.value || null })}
            />
          </label>
          <label>
            Ranking priorities
            <textarea
              rows={3}
              maxLength={4000}
              value={definition.rankingPriorities}
              onChange={(e) => patch({ rankingPriorities: e.target.value })}
            />
          </label>
        </details>
        <details>
          <summary>Resource limits</summary>
          <div className="mission-budget-grid">
            {number("maxActions", "Managed actions", 20, 2000)}
            {number("deadlineHours", "Elapsed deadline (hours)", 1, 720)}
            <label>
              Active execution hours
              <input
                type="number"
                min={1}
                max={168}
                value={definition.activeSeconds / 3600}
                onChange={(e) =>
                  patch({ activeSeconds: Number(e.target.value) * 3600 })
                }
              />
            </label>
            <label>
              Minutes per action
              <input
                type="number"
                min={1}
                max={120}
                value={definition.actionTimeoutSeconds / 60}
                onChange={(e) =>
                  patch({ actionTimeoutSeconds: Number(e.target.value) * 60 })
                }
              />
            </label>
          </div>
          <p className="task-muted">
            Limits cover the entire portfolio, including revisions and
            replacements. Action counts are not a dollar cap; provider charges
            and account limits still apply.
          </p>
        </details>
      </fieldset>
      {error && (
        <p className="task-error" role="alert">
          {error}
        </p>
      )}
      <button
        className="task-primary"
        disabled={busy || Boolean(discoveryError(definition))}
      >
        {busy ? "Starting…" : "Start self-discovery"}
      </button>
      <p className="task-muted">
        {definition.mode === "supervised"
          ? "The automation pauses for your project selection, then continues through the papers."
          : "The automation makes research decisions without follow-up questions."}{" "}
        Keep Pipeline running and the computer awake. Paper count is a target;
        unavailable evidence and incomplete research remain explicit.
      </p>
    </form>
  );
}
