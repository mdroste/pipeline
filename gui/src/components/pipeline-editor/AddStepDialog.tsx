import { invoke } from "@tauri-apps/api/core";
import { memo, useEffect, useId, useMemo, useRef, useState } from "react";
import type {
  AutoReviewCatalog,
  AutoReviewCatalogRole,
  Phase,
} from "../../lib/types";
import useModalDialog from "../../hooks/useModalDialog";

export type StepContextPreset = "standard" | "input" | "prior_reports" | "isolated";

export interface AddStepDraft {
  mode: "guided" | "adaptive" | "blank";
  label: string;
  instructions: string;
  phase: Phase;
  contextPreset: StepContextPreset;
  output: "report" | "issues";
  templateId?: string;
}

type AdaptiveCatalogTab = "subjects" | "methods";

interface ListedRole {
  role: AutoReviewCatalogRole;
  group: string;
}

function ChoiceCard({
  name,
  value,
  checked,
  disabled,
  title,
  description,
  onChange,
}: {
  name: string;
  value: string;
  checked: boolean;
  disabled?: boolean;
  title: string;
  description: string;
  onChange: () => void;
}) {
  return (
    <label className={`flex cursor-pointer items-start gap-2.5 rounded-lg border p-3 transition-colors ${
      checked
        ? "border-gray-700 bg-gray-50 dark:border-gray-300 dark:bg-gray-800"
        : "border-gray-200 hover:border-gray-400 dark:border-gray-700 dark:hover:border-gray-500"
    } ${disabled ? "cursor-not-allowed opacity-40" : ""}`}>
      <input
        type="radio"
        name={name}
        value={value}
        checked={checked}
        disabled={disabled}
        onChange={onChange}
        className="mt-0.5"
      />
      <span>
        <span className="block text-xs font-medium text-gray-800 dark:text-gray-200">{title}</span>
        <span className="mt-0.5 block text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
          {description}
        </span>
      </span>
    </label>
  );
}

function roleMatches(role: AutoReviewCatalogRole, query: string): boolean {
  if (!query) return true;
  return `${role.label} ${role.description} ${role.exclusions}`
    .toLowerCase()
    .includes(query);
}

function AdaptiveRoleCard({
  listed,
  selected,
  onSelect,
}: {
  listed: ListedRole;
  selected: boolean;
  onSelect: () => void;
}) {
  const { role, group } = listed;
  return (
    <button
      type="button"
      aria-label={role.label}
      aria-pressed={selected}
      onClick={onSelect}
      className={`w-full rounded-xl border p-3 text-left transition-colors ${
        selected
          ? "border-blue-500 bg-blue-50 ring-2 ring-blue-100 dark:border-blue-400 dark:bg-blue-950/40 dark:ring-blue-950"
          : "border-gray-200 bg-white hover:border-gray-400 hover:bg-gray-50 dark:border-gray-700 dark:bg-gray-900 dark:hover:border-gray-500 dark:hover:bg-gray-800"
      }`}
    >
      <span className="flex items-start justify-between gap-3">
        <span className="text-sm font-medium text-gray-900 dark:text-gray-100">{role.label}</span>
        <span className="shrink-0 rounded-full bg-gray-100 px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-gray-500 dark:bg-gray-800 dark:text-gray-400">
          {role.level === "discipline" ? "Broad" : group}
        </span>
      </span>
      <span className="mt-1 block text-[11px] leading-4 text-gray-600 dark:text-gray-300">
        {role.description}
      </span>
      {selected && (
        <span className="mt-2 block border-t border-blue-200 pt-2 text-[10px] leading-4 text-gray-500 dark:border-blue-900 dark:text-gray-400">
          <span className="font-medium">Scope boundary:</span> {role.exclusions}
        </span>
      )}
    </button>
  );
}

function AddStepDialog({
  onCreate,
  onCancel,
}: {
  onCreate: (draft: AddStepDraft) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [mode, setMode] = useState<AddStepDraft["mode"]>("guided");
  const [label, setLabel] = useState("");
  const [instructions, setInstructions] = useState("");
  const [phase, setPhase] = useState<Phase>("parallel");
  const [contextPreset, setContextPreset] = useState<StepContextPreset>("standard");
  const [output, setOutput] = useState<"report" | "issues">("report");
  const [catalog, setCatalog] = useState<AutoReviewCatalog | null>(null);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [catalogAttempt, setCatalogAttempt] = useState(0);
  const [catalogTab, setCatalogTab] = useState<AdaptiveCatalogTab>("subjects");
  const [query, setQuery] = useState("");
  const [selectedDiscipline, setSelectedDiscipline] = useState("");
  const [selectedRoleId, setSelectedRoleId] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const adaptiveSearchRef = useRef<HTMLInputElement>(null);
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onCancel);

  useEffect(() => {
    if (mode !== "adaptive" || catalog) return;
    let stale = false;
    setCatalogLoading(true);
    setCatalogError(null);
    invoke<AutoReviewCatalog>("get_auto_review_catalog")
      .then((value) => {
        if (stale) return;
        setCatalog(value);
        setSelectedDiscipline(
          value.disciplines.find((discipline) => discipline.id === "economics")?.id
            ?? value.disciplines[0]?.id
            ?? "",
        );
      })
      .catch((caught) => {
        if (!stale) {
          setCatalogError(caught instanceof Error ? caught.message : String(caught));
        }
      })
      .finally(() => {
        if (!stale) setCatalogLoading(false);
      });
    return () => {
      stale = true;
    };
  }, [mode, catalog, catalogAttempt]);

  useEffect(() => {
    if (mode !== "adaptive") return;
    const timer = window.setTimeout(() => adaptiveSearchRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, [mode]);

  const normalizedQuery = query.trim().toLowerCase();
  const listedRoles = useMemo<ListedRole[]>(() => {
    if (!catalog) return [];
    if (catalogTab === "methods") {
      return catalog.methodFamilies.flatMap((family) => family.roles
        .filter((role) => roleMatches(role, normalizedQuery))
        .map((role) => ({ role, group: family.label })));
    }
    const disciplines = normalizedQuery
      ? catalog.disciplines
      : catalog.disciplines.filter((discipline) => discipline.id === selectedDiscipline);
    return disciplines.flatMap((discipline) => discipline.roles
      .filter((role) => roleMatches(role, normalizedQuery))
      .map((role) => ({ role, group: discipline.label })));
  }, [catalog, catalogTab, normalizedQuery, selectedDiscipline]);

  const selectedRole = useMemo(() => {
    if (!catalog || !selectedRoleId) return null;
    return catalog.methodFamilies
      .flatMap((family) => family.roles)
      .find((role) => role.id === selectedRoleId)
      ?? catalog.disciplines
        .flatMap((discipline) => discipline.roles)
        .find((role) => role.id === selectedRoleId)
      ?? null;
  }, [catalog, selectedRoleId]);

  const choosePhase = (next: Phase) => {
    setPhase(next);
    if (next === "parallel" && contextPreset === "prior_reports") {
      setContextPreset("standard");
    }
  };
  const canCreate = mode === "adaptive"
    ? !!selectedRole
    : !!label.trim() && (mode === "blank" || !!instructions.trim());

  const submit = async () => {
    if (!canCreate || submitting) return;
    setSubmitting(true);
    setSubmitError(null);
    try {
      await onCreate({
        mode,
        label: mode === "adaptive" ? selectedRole?.label ?? "" : label.trim(),
        instructions: instructions.trim(),
        phase: mode === "adaptive" ? "parallel" : phase,
        contextPreset,
        output,
        templateId: mode === "adaptive" ? selectedRole?.id : undefined,
      });
    } catch (caught) {
      setSubmitError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/45 p-5"
      onMouseDown={(event) => {
        // Close only on a press that starts on the backdrop itself — a click
        // handler would also fire when a text-selection drag that began inside
        // the dialog is released over the backdrop, discarding the draft.
        if (event.target === event.currentTarget) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-gray-200 bg-white shadow-2xl dark:border-gray-700 dark:bg-gray-900"
      >
        <div className="border-b border-gray-200 px-6 pt-5 dark:border-gray-700">
          <h2 id={titleId} className="text-base font-semibold text-gray-900 dark:text-gray-100">Add workflow step</h2>
          <p id={descriptionId} className="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Design a new step or add a configurable agent from the adaptive review suite.
          </p>
          <div role="tablist" aria-label="Add step method" className="mt-4 flex gap-5">
            <button
              type="button"
              role="tab"
              aria-selected={mode === "guided"}
              onClick={() => setMode("guided")}
              className={`border-b-2 pb-2 text-xs font-medium ${mode === "guided" ? "border-gray-900 text-gray-900 dark:border-gray-100 dark:text-gray-100" : "border-transparent text-gray-500"}`}
            >
              Guided setup
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "adaptive"}
              onClick={() => setMode("adaptive")}
              className={`border-b-2 pb-2 text-xs font-medium ${mode === "adaptive" ? "border-blue-600 text-blue-700 dark:border-blue-400 dark:text-blue-300" : "border-transparent text-gray-500"}`}
            >
              Adaptive agent
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "blank"}
              onClick={() => setMode("blank")}
              className={`border-b-2 pb-2 text-xs font-medium ${mode === "blank" ? "border-gray-900 text-gray-900 dark:border-gray-100 dark:text-gray-100" : "border-transparent text-gray-500"}`}
            >
              Blank step
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-5">
          {mode === "adaptive" ? (
            <div className="space-y-4">
              <div className="rounded-xl border border-blue-100 bg-blue-50/70 px-4 py-3 dark:border-blue-900 dark:bg-blue-950/30">
                <p className="text-xs font-medium text-blue-950 dark:text-blue-100">Choose an adaptive agent</p>
                <p className="mt-1 text-[11px] leading-4 text-blue-800 dark:text-blue-200">
                  Choose a subject or method specialist. Its complete prompt and execution settings are copied into this workflow, where you can configure them like any other step.
                </p>
              </div>

              {catalogLoading && !catalog && (
                <p role="status" className="rounded-lg border border-gray-200 p-6 text-center text-sm text-gray-500 dark:border-gray-700">
                  Loading adaptive agents…
                </p>
              )}
              {catalogError && !catalog && (
                <div role="alert" className="rounded-lg border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
                  <p>Could not load the adaptive-agent catalog: {catalogError}</p>
                  <button
                    type="button"
                    onClick={() => setCatalogAttempt((attempt) => attempt + 1)}
                    className="mt-2 text-xs font-medium underline underline-offset-2"
                  >
                    Try again
                  </button>
                </div>
              )}

              {catalog && (
                <>
                  <div className="flex flex-col gap-3 sm:flex-row sm:items-center">
                    <div className="inline-flex w-fit rounded-lg bg-gray-100 p-1 dark:bg-gray-800" role="tablist" aria-label="Adaptive agent type">
                      <button
                        type="button"
                        role="tab"
                        aria-selected={catalogTab === "subjects"}
                        onClick={() => setCatalogTab("subjects")}
                        className={`rounded-md px-3 py-1.5 text-xs font-medium ${catalogTab === "subjects" ? "bg-white text-gray-950 shadow-sm dark:bg-gray-700 dark:text-white" : "text-gray-500"}`}
                      >
                        Subjects ({catalog.subjectCount})
                      </button>
                      <button
                        type="button"
                        role="tab"
                        aria-selected={catalogTab === "methods"}
                        onClick={() => setCatalogTab("methods")}
                        className={`rounded-md px-3 py-1.5 text-xs font-medium ${catalogTab === "methods" ? "bg-white text-gray-950 shadow-sm dark:bg-gray-700 dark:text-white" : "text-gray-500"}`}
                      >
                        Methods ({catalog.methodCount})
                      </button>
                    </div>
                    {catalogTab === "subjects" && !normalizedQuery && (
                      <label className="min-w-0 flex-1">
                        <span className="sr-only">Subject discipline</span>
                        <select
                          aria-label="Subject discipline"
                          value={selectedDiscipline}
                          onChange={(event) => setSelectedDiscipline(event.target.value)}
                          className="w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs text-gray-900 outline-none focus:ring-2 focus:ring-blue-100 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100"
                        >
                          {catalog.disciplines.map((discipline) => (
                            <option key={discipline.id} value={discipline.id}>
                              {discipline.label} ({discipline.roles.length})
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    <label className="min-w-0 flex-1">
                      <span className="sr-only">Search adaptive agents</span>
                      <input
                        ref={adaptiveSearchRef}
                        type="search"
                        value={query}
                        onChange={(event) => setQuery(event.target.value)}
                        placeholder="Search e.g. macro, causal, proof…"
                        className="w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs text-gray-900 outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-100 dark:border-gray-700 dark:bg-gray-800 dark:text-gray-100 dark:focus:ring-blue-950"
                      />
                    </label>
                  </div>

                  <div className="max-h-[42vh] overflow-y-auto rounded-xl border border-gray-200 bg-gray-50 p-2 dark:border-gray-700 dark:bg-gray-950">
                    {listedRoles.length ? (
                      <div className="grid gap-2 md:grid-cols-2">
                        {listedRoles.map((listed) => (
                          <AdaptiveRoleCard
                            key={listed.role.id}
                            listed={listed}
                            selected={selectedRoleId === listed.role.id}
                            onSelect={() => setSelectedRoleId(listed.role.id)}
                          />
                        ))}
                      </div>
                    ) : (
                      <p className="p-8 text-center text-sm text-gray-500 dark:text-gray-400">
                        No adaptive agents match this search.
                      </p>
                    )}
                  </div>
                </>
              )}
            </div>
          ) : (
            <div className="space-y-5">
              <div>
                <label htmlFor="new-step-name" className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                  Step name
                </label>
                <input
                  id="new-step-name"
                  data-autofocus
                  value={label}
                  onChange={(event) => setLabel(event.target.value)}
                  placeholder="e.g. Check identification strategy"
                  className="mt-1.5 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-900 focus:outline-none focus:ring-2 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                />
              </div>

              {mode === "guided" && (
                <div>
                  <label htmlFor="new-step-instructions" className="block text-xs font-medium text-gray-700 dark:text-gray-300">
                    What should this step do?
                  </label>
                  <textarea
                    id="new-step-instructions"
                    value={instructions}
                    onChange={(event) => setInstructions(event.target.value)}
                    rows={4}
                    placeholder="Describe the judgment or analysis you want from the model."
                    className="mt-1.5 w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm leading-relaxed text-gray-900 focus:outline-none focus:ring-2 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                  />
                </div>
              )}

              <fieldset>
                <legend className="mb-2 text-xs font-medium text-gray-700 dark:text-gray-300">When can it run?</legend>
                <div className="grid gap-2 sm:grid-cols-2">
                  <ChoiceCard
                    name="new-step-phase"
                    value="parallel"
                    checked={phase === "parallel"}
                    title="Independently"
                    description="Run alongside other ready steps. It cannot consume their output."
                    onChange={() => choosePhase("parallel")}
                  />
                  <ChoiceCard
                    name="new-step-phase"
                    value="sequential"
                    checked={phase === "sequential"}
                    title="After earlier steps"
                    description="May wait for and read reports or files produced upstream."
                    onChange={() => choosePhase("sequential")}
                  />
                </div>
              </fieldset>

              {mode === "guided" && (
                <>
                  <fieldset>
                    <legend className="mb-2 text-xs font-medium text-gray-700 dark:text-gray-300">What should it receive?</legend>
                    <div className="grid gap-2 sm:grid-cols-2">
                      <ChoiceCard
                        name="new-step-context"
                        value="standard"
                        checked={contextPreset === "standard"}
                        title="Recommended context"
                        description={phase === "parallel" ? "Primary input and orientation map." : "Earlier reports and orientation map."}
                        onChange={() => setContextPreset("standard")}
                      />
                      <ChoiceCard
                        name="new-step-context"
                        value="input"
                        checked={contextPreset === "input"}
                        title="Primary input only"
                        description="Read the selected document or folder, without the orientation map."
                        onChange={() => setContextPreset("input")}
                      />
                      <ChoiceCard
                        name="new-step-context"
                        value="prior_reports"
                        checked={contextPreset === "prior_reports"}
                        disabled={phase === "parallel"}
                        title="Earlier reports only"
                        description="Available for steps that run after earlier work."
                        onChange={() => setContextPreset("prior_reports")}
                      />
                      <ChoiceCard
                        name="new-step-context"
                        value="isolated"
                        checked={contextPreset === "isolated"}
                        title="No workflow artifacts"
                        description="Run only from the instructions written in the prompt."
                        onChange={() => setContextPreset("isolated")}
                      />
                    </div>
                  </fieldset>

                  <fieldset>
                    <legend className="mb-2 text-xs font-medium text-gray-700 dark:text-gray-300">What should it produce?</legend>
                    <div className="grid gap-2 sm:grid-cols-2">
                      <ChoiceCard
                        name="new-step-output"
                        value="report"
                        checked={output === "report"}
                        title="Markdown report"
                        description="A readable analysis for the final workflow output."
                        onChange={() => setOutput("report")}
                      />
                      <ChoiceCard
                        name="new-step-output"
                        value="issues"
                        checked={output === "issues"}
                        title="Structured issues"
                        description="Validated JSON that powers the Issues table and annotations."
                        onChange={() => setOutput("issues")}
                      />
                    </div>
                  </fieldset>
                </>
              )}

              {mode === "blank" && (
                <p className="rounded-lg bg-gray-50 p-3 text-xs leading-relaxed text-gray-600 dark:bg-gray-800 dark:text-gray-400">
                  A starter prompt and the standard context for the selected phase will be added. The new step opens immediately so you can configure its prompt, inputs, execution rules, models, and agents.
                </p>
              )}
            </div>
          )}
        </div>

        <div className="border-t border-gray-200 px-6 py-4 dark:border-gray-700">
          {submitError && (
            <p role="alert" className="mb-3 text-xs text-red-700 dark:text-red-300">
              Could not copy this step: {submitError}
            </p>
          )}
          <div className="flex items-center justify-between gap-3">
            <p className="text-[11px] text-gray-500 dark:text-gray-400">
              {mode === "adaptive"
                ? "This adds an editable snapshot. Workflow changes never alter the adaptive suite default."
                : "All choices remain editable after creation."}
            </p>
            <div className="flex shrink-0 gap-2">
              <button type="button" onClick={onCancel} className="rounded-lg px-3 py-2 text-sm text-gray-600 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-800">
                Cancel
              </button>
              <button
                type="button"
                disabled={!canCreate || submitting}
                onClick={() => void submit()}
                className="rounded-lg bg-gray-900 px-4 py-2 text-sm font-medium text-white hover:bg-gray-800 disabled:bg-gray-300 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-gray-200 dark:disabled:bg-gray-700"
              >
                {submitting ? "Copying…" : mode === "adaptive" ? "Copy step" : "Create step"}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export default memo(AddStepDialog);
