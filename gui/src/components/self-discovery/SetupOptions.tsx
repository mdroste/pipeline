import { AllowanceField } from "../tasks/Budget";
// Optional discovery controls edit the same draft as the brief and portfolio targets.
import type { ReactNode } from "react";
import { Icon, type IconName } from "../../ui/icons";
import type { DiscoveryDefinition } from "../../lib/discoveryClient";

type Patch = (update: Partial<DiscoveryDefinition>) => void;
type CountKey = Exclude<
  {
    [K in keyof DiscoveryDefinition]-?: DiscoveryDefinition[K] extends number
      ? K
      : never;
  }[keyof DiscoveryDefinition],
  "schemaVersion"
>;

export function CountField({
  label,
  value,
  min,
  max,
  onChange,
  hint,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  hint?: string;
}) {
  return (
    <label className="ds-count">
      <span>
        {label}
        {hint && <small aria-hidden="true">{hint}</small>}
      </span>
      <input
        type="number"
        required
        min={min}
        max={max}
        aria-label={label}
        aria-description={hint}
        value={Number.isFinite(value) ? value : ""}
        onChange={(e) => onChange(e.target.valueAsNumber)}
      />
    </label>
  );
}

function Group({
  title,
  summary,
  icon,
  children,
}: {
  title: string;
  summary: string;
  icon: IconName;
  children: ReactNode;
}) {
  return (
    <details className="ds-group">
      <summary>
        <Icon name={icon} />
        <span>
          {title}
          <small>{summary}</small>
        </span>
        <Icon name="chevron-down" className="ds-chevron" />
      </summary>
      <div className="ds-group-body">{children}</div>
    </details>
  );
}

export default function SetupOptions({
  definition: d,
  patch,
  hasProject,
  profiles,
}: {
  definition: DiscoveryDefinition;
  patch: Patch;
  hasProject: boolean;
  profiles: { id: string; name: string }[];
}) {
  const count = (
    key: CountKey,
    label: string,
    min: number,
    max: number,
    scale = 1,
  ) => (
    <CountField
      label={label}
      min={min}
      max={max}
      value={d[key] / scale}
      onChange={(v) => patch({ [key]: v * scale })}
    />
  );
  return (
    <section className="ds-options" aria-label="Optional settings">
      <h3>Optional settings</h3>
      <Group
        title="Sources & tools"
        icon="folder"
        summary={`Literature search ${d.acquireLiterature ? "on" : "off"} · Computation ${d.allowComputation ? "on" : "off"}`}
      >
        <label className="ds-field">
          Research input files
          <textarea
            rows={3}
            disabled={!hasProject}
            placeholder={
              hasProject
                ? "One file path per line, relative to the project folder"
                : "Choose an existing project to include files"
            }
            value={d.inputPaths.join("\n")}
            onChange={(e) => patch({ inputPaths: e.target.value.split("\n") })}
          />
        </label>
        <p>
          Selected files are copied into each paper’s folder at the start and
          made available to its research models.
        </p>
        <label className="ds-check">
          <input
            type="checkbox"
            checked={d.acquireLiterature}
            onChange={(e) => patch({ acquireLiterature: e.target.checked })}
          />
          Search public literature records and available abstracts
        </label>
        <p>
          Results may not include full text. For an existing project, enable
          literature search in Sources first.
        </p>
        <label className="ds-check">
          <input
            type="checkbox"
            checked={d.allowComputation}
            onChange={(e) => patch({ allowComputation: e.target.checked })}
          />
          Allow code and computation in isolated paper folders
        </label>
        <p>
          Commands use installed tools without network access. Original project
          files stay outside the paper folders.
        </p>
      </Group>
      <Group
        title="Research & review"
        icon="review"
        summary={
          d.ideaAgents
            ? `${d.ideaAgents} idea subagents · Reviewers and investigation rounds`
            : "Idea subagents, reviewers, and investigation rounds"
        }
      >
        <CountField
          label="Idea subagents"
          min={0}
          max={8}
          value={d.ideaAgents ?? 0}
          onChange={(ideaAgents) => patch({ ideaAgents })}
        />
        <p>
          Choose 1–8 to have ChatGPT or Claude Code divide the topic among
          subagents once, before proposals are developed. Use 0 to skip
          delegation.
        </p>
        <div className="ds-count-grid">
          {count("proposalReviewers", "Detailed proposal reviewers", 0, 4)}
          {count("proposalRevisionPasses", "Proposal revision passes", 0, 2)}
          {count("paperReviewers", "Paper reviewers per round", 0, 4)}
          {count("revisionPasses", "Revision passes", 0, 5)}
          {count("researchRounds", "Investigation rounds per paper", 1, 16)}
          {count(
            "maxReplacements",
            "Maximum projects Pipeline can replace automatically",
            0,
            5,
          )}
        </div>
        <label className="ds-check">
          <input
            type="checkbox"
            checked={d.challengeResearch}
            onChange={(e) => patch({ challengeResearch: e.target.checked })}
          />
          Challenge findings after each research round
        </label>
        <label className="ds-field">
          Additional structured Review
          <select
            value={d.reviewProfileId ?? ""}
            onChange={(e) => patch({ reviewProfileId: e.target.value || null })}
          >
            <option value="">
              Use built-in reviewers selected for the research field
            </option>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </Group>
      <Group
        title="Models & ranking"
        icon="settings"
        summary="Author, reviewer, and ranking priorities"
      >
        <div className="ds-count-grid">
          <label className="ds-field">
            Author model
            <input
              value={d.authorModel ?? ""}
              placeholder="Current configured model"
              onChange={(e) => patch({ authorModel: e.target.value || null })}
            />
          </label>
          <label className="ds-field">
            Reviewer model
            <input
              value={d.reviewerModel ?? ""}
              placeholder="Current configured model"
              onChange={(e) => patch({ reviewerModel: e.target.value || null })}
            />
          </label>
        </div>
        <label className="ds-field">
          Ranking priorities
          <textarea
            rows={3}
            maxLength={4000}
            value={d.rankingPriorities}
            onChange={(e) => patch({ rankingPriorities: e.target.value })}
          />
        </label>
      </Group>
      <Group
        title="Resource limits"
        icon="history"
        summary={`Up to ${Number.isFinite(d.maxActions) ? d.maxActions : "…"} actions · ${Number.isFinite(d.activeSeconds) ? d.activeSeconds / 3600 : "…"} h running time`}
      >
        <div className="ds-count-grid">
          <AllowanceField
            cents={d.maxApiCostCents}
            onChange={(maxApiCostCents) => patch({ maxApiCostCents })}
          />
          {count("maxActions", "Maximum actions", 20, 2000)}
          {count("deadlineHours", "Finish within (hours)", 1, 720)}
          {count(
            "activeSeconds",
            "Maximum time spent running (hours)",
            1,
            168,
            3600,
          )}
          {count("actionTimeoutSeconds", "Minutes per action", 1, 120, 60)}
        </div>
        <p>
          Limits cover the entire portfolio, including revisions and
          replacements. Action counts are not a spending cap; provider charges
          and account limits still apply.
        </p>
      </Group>
    </section>
  );
}
