/** Full-width, tabbed setup keeps draft state and validation shared across sections. */
import { useEffect, useId, useRef, useState } from "react";
import { workflowProfileClient } from "../../lib/workflowProfileClient";
import { taskClient } from "../../lib/taskClient";
import { type SessionChoice } from "../../lib/taskTypes";
import {
  missionClient,
  newMission,
  type Mission,
  type MissionChoices,
  type MissionDefinition,
  type MissionMethod,
  type MissionMode,
} from "../../lib/missionClient";
import ReportIssueLink from "../ReportIssueLink";
import useTabList from "../../hooks/useTabList";
import { BudgetFields, ToolFields } from "./BuilderOptions";
import "./Builder.css";

const sections = ["research", "limits", "tools"] as const;
type Section = (typeof sections)[number];

const draftKey = "pipeline.researchMission.draft.v1";
interface MissionDraft {
  definition: MissionDefinition;
  sessionId: string;
  operationId: string;
}
/** Checks, experiments and methods belong to the conversation's project. */
function forSession(draft: MissionDraft, sessionId: string): MissionDraft {
  return {
    ...draft,
    sessionId,
    operationId: crypto.randomUUID(),
    definition: {
      ...draft.definition,
      policy: newMission().policy,
      methodIds: [],
    },
  };
}
function recover(sessionId?: string | null): MissionDraft {
  try {
    const saved = JSON.parse(localStorage.getItem(draftKey) ?? "null");
    if (
      saved?.definition?.schemaVersion === 1 &&
      typeof saved.sessionId === "string" &&
      typeof saved.operationId === "string"
    ) {
      // The conversation it was opened from outranks an earlier draft's.
      return sessionId && sessionId !== saved.sessionId
        ? forSession(saved, sessionId)
        : saved;
    }
  } catch {
    /* An unavailable local draft does not prevent preparation. */
  }
  return {
    definition: newMission(),
    sessionId: sessionId ?? "",
    operationId: crypto.randomUUID(),
  };
}
export default function MissionBuilder({
  initialSessionId,
  onChooseType,
  onPrepared,
  onClose,
}: {
  initialSessionId?: string | null;
  onChooseType?: () => void;
  onPrepared: (m: Mission) => void;
  onClose: () => void;
}) {
  const [section, setSection] = useState<Section>("research");
  const tabs = useTabList(sections, section, setSection);
  const heading = useRef<HTMLHeadingElement>(null);
  const [draft, setDraft] = useState(() => recover(initialSessionId));
  const [sessions, setSessions] = useState<SessionChoice[]>([]);
  const [choices, setChoices] = useState<MissionChoices | null>(null);
  const [methods, setMethods] = useState<MissionMethod[]>([]);
  const [profiles, setProfiles] = useState<{ id: string; name: string }[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const choiceVersion = useRef(0);
  const blockedId = useId();
  useEffect(() => {
    heading.current?.focus({ preventScroll: true });
    heading.current?.scrollIntoView?.({ block: "nearest" });
  }, []);
  useEffect(() => {
    let live = true;
    void Promise.all([taskClient.sessions(), workflowProfileClient.profiles()])
      .then(([s, p]) => {
        if (live) {
          setSessions(s.filter((v) => v.workspaceName !== null));
          setProfiles(p);
        }
      })
      .catch((e) => {
        if (live) setError(String(e));
      });
    return () => {
      live = false;
    };
  }, []);
  useEffect(() => {
    try {
      localStorage.setItem(draftKey, JSON.stringify(draft));
    } catch {
      /* The prepared mission is persisted natively. */
    }
  }, [draft]);
  useEffect(() => {
    const version = ++choiceVersion.current;
    setChoices(null);
    setMethods([]);
    if (draft.sessionId)
      void missionClient
        .choices(draft.sessionId)
        .then(async (c) => {
          if (version !== choiceVersion.current) return;
          setChoices(c);
          const available = await missionClient.methods(c.workspaceId);
          if (version === choiceVersion.current) setMethods(available);
        })
        .catch((e) => {
          if (version === choiceVersion.current) setError(String(e));
        });
    return () => {
      choiceVersion.current++;
    };
  }, [draft.sessionId]);
  function patch(update: Partial<MissionDefinition>) {
    setDraft((d) => ({
      ...d,
      definition: { ...d.definition, ...update },
      operationId: crypto.randomUUID(),
    }));
  }
  const d = draft.definition;
  // Criteria keep their raw lines while typing; blank lines are not criteria.
  const criteria = d.criteria.map((c) => c.trim()).filter(Boolean);
  const blocked = !draft.sessionId
    ? "Choose a project conversation."
    : !choices
      ? error
        ? "The conversation’s project settings could not be loaded."
        : "Loading the conversation’s project settings…"
      : !d.name.trim()
        ? "Name the automation."
        : !d.objective.trim()
          ? "Describe the research question and scope."
          : !criteria.length
            ? "Add at least one completion criterion."
            : "";
  async function prepare() {
    if (blocked || busy) return;
    setBusy(true);
    setError("");
    try {
      const m = await missionClient.prepare(
        { ...d, criteria },
        draft.sessionId,
        draft.operationId,
      );
      localStorage.removeItem(draftKey);
      onPrepared(m);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form
      className="mission-builder"
      noValidate
      aria-label="New research automation"
      onSubmit={(e) => {
        e.preventDefault();
        // Reveal an invalid field before asking the browser to focus it.
        const invalid = e.currentTarget.querySelector<HTMLInputElement>(
          "input:invalid, select:invalid, textarea:invalid",
        );
        if (invalid) {
          const target = invalid.closest<HTMLElement>("[data-builder-section]")
            ?.dataset.builderSection;
          if (target) setSection(target as Section);
          requestAnimationFrame(() => {
            invalid.focus();
            invalid.reportValidity();
          });
          return;
        }
        void prepare();
      }}
    >
      <div className="task-section-heading mission-builder-heading">
        <h2 ref={heading} tabIndex={-1}>
          New research automation
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
            aria-label="Close research automation builder"
          >
            ×
          </button>
        </div>
      </div>
      <fieldset disabled={busy} className="mission-fields">
        <div className="mission-builder-context">
          <label>
            Source conversation
            <select
              value={draft.sessionId}
              onChange={(e) => {
                const sessionId = e.target.value;
                setError("");
                setDraft((old) => forSession(old, sessionId));
              }}
            >
              <option value="">Choose a project conversation</option>
              {sessions.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.title} · {s.workspaceName}
                </option>
              ))}
            </select>
          </label>
          <label>
            Starting point
            <select
              value={d.mode}
              onChange={(e) => {
                const next = newMission(e.target.value as MissionMode);
                patch({
                  mode: next.mode,
                  name: next.name,
                  objective: next.objective,
                  criteria: next.criteria,
                });
              }}
            >
              {(
                [
                  "theory",
                  "empirical",
                  "quantitative",
                  "literature",
                  "discovery",
                  "maintenance",
                ] as const
              ).map((v) => (
                <option key={v} value={v}>
                  {
                    {
                      theory: "Theory investigation",
                      empirical: "Empirical investigation",
                      quantitative: "Quantitative model",
                      literature: "Literature investigation",
                      discovery: "Research discovery",
                      maintenance: "Project maintenance",
                    }[v]
                  }
                </option>
              ))}
            </select>
          </label>
          <label>
            Automation name
            <input
              maxLength={160}
              value={d.name}
              onChange={(e) => patch({ name: e.target.value })}
            />
          </label>
        </div>
        <div
          className="ui-page-tabs mission-builder-tabs"
          role="tablist"
          aria-label="Research automation settings"
        >
          {sections.map((id) => (
            <button key={id} {...tabs.tabProps(id)}>
              {
                {
                  research: "Research",
                  limits: "Limits",
                  tools: "Tools & review",
                }[id]
              }
            </button>
          ))}
        </div>
        <div
          {...tabs.panelProps("research")}
          hidden={section !== "research"}
          data-builder-section="research"
        >
          <div className="mission-builder-columns">
            <section className="mission-builder-card mission-builder-stack">
              <label>
                Research question and scope
                <textarea
                  rows={4}
                  value={d.objective}
                  onChange={(e) => patch({ objective: e.target.value })}
                  maxLength={8000}
                />
              </label>
              <label>
                Current understanding
                <textarea
                  rows={2}
                  value={d.background}
                  onChange={(e) => patch({ background: e.target.value })}
                  placeholder="What is known, what has failed, and any important constraints"
                  maxLength={16000}
                />
              </label>
            </section>
            <section className="mission-builder-card mission-builder-stack">
              <label className="mission-builder-criteria">
                Completion criteria{" "}
                <span className="task-muted">One per line</span>
                <textarea
                  rows={9}
                  value={d.criteria.join("\n")}
                  onChange={(e) =>
                    patch({ criteria: e.target.value.split("\n") })
                  }
                />
              </label>
            </section>
          </div>
        </div>
        <div
          {...tabs.panelProps("limits")}
          hidden={section !== "limits"}
          data-builder-section="limits"
        >
          <BudgetFields definition={d} onChange={patch} />
        </div>
        <div
          {...tabs.panelProps("tools")}
          hidden={section !== "tools"}
          data-builder-section="tools"
        >
          <ToolFields
            definition={d}
            onChange={patch}
            choices={choices}
            methods={methods}
            profiles={profiles}
          />
        </div>
      </fieldset>
      <footer className="automation-builder-footer mission-builder-footer">
        <div className="mission-builder-status">
          {error && (
            <p role="alert" className="task-error">
              {error}
              <ReportIssueLink />
            </p>
          )}
          {blocked ? (
            <p id={blockedId} className="task-muted">
              {blocked}
            </p>
          ) : (
            <p className="task-muted">
              {d.budget.maxRounds} rounds · {d.budget.maxActions} actions ·{" "}
              {d.budget.activeSeconds / 3600} active hours
            </p>
          )}
          <p className="task-muted">
            Review the plan and permissions before starting. Keep Pipeline
            running and this computer awake.
          </p>
        </div>
        <button
          type="submit"
          className="ui-button ui-button--primary"
          disabled={busy || Boolean(blocked)}
          aria-describedby={blocked ? blockedId : undefined}
        >
          {busy ? "Preparing…" : "Preview research automation →"}
        </button>
      </footer>
    </form>
  );
}
