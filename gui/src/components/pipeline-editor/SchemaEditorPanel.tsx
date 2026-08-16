import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AutoReviewCatalog, StepConfig } from "../../lib/types";
import { AUTO_REVIEW_CONTRACT } from "../../lib/autoReview";
import { ISSUES_SCHEMA } from "./stepTemplates";
import { outputSchemaError } from "./utils";

// UI rendering of crate::output::text_artifact_schema(). Runtime validation
// remains host-owned; keep this preview in lockstep when that contract changes.
export const TEXT_ARTIFACT_SCHEMA = {
  type: "object",
  required: ["content"],
  properties: {
    content: { type: "string" },
  },
};

const DEFAULT_ORIENTATION_SCHEMA = { type: "object" };
const STARTER_JSON_SCHEMA = {
  type: "object",
  required: ["result"],
  properties: {
    result: { type: "string" },
  },
};

export type SchemaEditorTarget =
  | {
      kind: "orientation";
      schema?: Record<string, unknown> | null;
      autoReview: boolean;
    }
  | {
      kind: "step";
      step: StepConfig;
      publishesFindings: boolean;
    };

function schemaText(schema: Record<string, unknown>): string {
  return JSON.stringify(schema, null, 2);
}

export default function SchemaEditorPanel({
  target,
  scopeKey,
  onOrientationSchemaChange,
  onStepSchemaChange,
  onDraftValidityChange,
  onBrowseCatalog,
}: {
  target: SchemaEditorTarget;
  scopeKey: string;
  onOrientationSchemaChange: (schema: Record<string, unknown> | null) => void;
  onStepSchemaChange: (id: string, schema: Record<string, unknown> | null) => void;
  onDraftValidityChange: (valid: boolean) => void;
  onBrowseCatalog?: (tab: "subjects" | "methods") => void;
}) {
  const targetKey = target.kind === "orientation" ? "orientation" : target.step.id;
  const draftKey = `${scopeKey}:${targetKey}`;
  const configuredSchema = target.kind === "orientation"
    ? target.schema ?? null
    : target.step.output_schema ?? null;
  const [draft, setDraft] = useState(configuredSchema ? schemaText(configuredSchema) : "");
  const [error, setError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<AutoReviewCatalog | null>(null);
  const [resolvedSchema, setResolvedSchema] = useState<Record<string, unknown> | null>(null);
  const [resolutionError, setResolutionError] = useState<string | null>(null);
  const [resolving, setResolving] = useState(false);

  useEffect(() => {
    setDraft(configuredSchema ? schemaText(configuredSchema) : "");
    setError(null);
    setResolvedSchema(null);
    setResolutionError(null);
    onDraftValidityChange(true);
    // A target change remounts the editing draft. Successful edits remain
    // locally formatted without cursor jumps from parent object identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftKey]);

  useEffect(() => () => onDraftValidityChange(true), [onDraftValidityChange]);

  const catalogBacked = target.kind === "orientation" && target.autoReview && !!configuredSchema;

  useEffect(() => {
    if (!catalogBacked) {
      setCatalog(null);
      return;
    }
    let live = true;
    invoke<AutoReviewCatalog>("get_auto_review_catalog")
      .then((value) => {
        if (live) setCatalog(value);
      })
      .catch(() => {
        if (live) setCatalog(null);
      });
    return () => {
      live = false;
    };
  }, [catalogBacked]);

  const applyConfiguredSchema = (schema: Record<string, unknown> | null) => {
    setDraft(schema ? schemaText(schema) : "");
    setError(null);
    onDraftValidityChange(true);
    if (target.kind === "orientation") {
      onOrientationSchemaChange(schema);
    } else {
      onStepSchemaChange(target.step.id, schema);
    }
  };

  const changeDraft = (next: string) => {
    setDraft(next);
    setResolvedSchema(null);
    setResolutionError(null);
    if (!next.trim()) {
      if (target.kind === "orientation" && target.autoReview) {
        setError("Automatic Paper Review requires its generated orientation schema.");
        onDraftValidityChange(false);
        return;
      }
      setError(null);
      onDraftValidityChange(true);
      if (target.kind === "orientation") {
        onOrientationSchemaChange(null);
      } else {
        onStepSchemaChange(target.step.id, null);
      }
      return;
    }
    try {
      const parsed = JSON.parse(next) as Record<string, unknown>;
      const validationError = outputSchemaError(parsed);
      if (validationError) {
        setError(validationError);
        onDraftValidityChange(false);
        return;
      }
      if (target.kind === "step" &&
          (parsed["x-pipeline-contract"] !== undefined ||
           parsed["x-pipeline-adaptive-agent-count"] !== undefined)) {
        setError("x-pipeline contract settings are reserved for the orientation schema.");
        onDraftValidityChange(false);
        return;
      }
      if (target.kind === "orientation" && target.autoReview &&
          parsed["x-pipeline-contract"] !== AUTO_REVIEW_CONTRACT) {
        setError(
          `Automatic Paper Review must retain x-pipeline-contract: "${AUTO_REVIEW_CONTRACT}". ` +
          "Create a separate custom workflow to replace this host-owned contract.",
        );
        onDraftValidityChange(false);
        return;
      }
      setError(null);
      onDraftValidityChange(true);
      if (target.kind === "orientation") {
        onOrientationSchemaChange(parsed);
      } else {
        onStepSchemaChange(target.step.id, parsed);
      }
    } catch (parseError) {
      setError(parseError instanceof Error ? parseError.message : "invalid JSON");
      onDraftValidityChange(false);
    }
  };

  const toggleResolvedSchema = async () => {
    if (resolvedSchema) {
      setResolvedSchema(null);
      return;
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(draft) as Record<string, unknown>;
    } catch {
      setResolutionError("Fix the JSON draft before resolving its catalog references.");
      return;
    }
    setResolving(true);
    setResolutionError(null);
    try {
      const resolved = await invoke<Record<string, unknown>>(
        "resolve_orientation_schema_catalogs",
        { schema: parsed },
      );
      setResolvedSchema(resolved);
    } catch (caught) {
      setResolutionError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setResolving(false);
    }
  };

  const title = target.kind === "orientation"
    ? "Orientation map schema"
    : `${target.step.label} output schema`;
  const isManagedText = target.kind === "step" && !configuredSchema;
  const isDefaultOrientation = target.kind === "orientation" && !configuredSchema;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-gray-200 px-5 py-4 dark:border-gray-700">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">{title}</h2>
          <span className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
            configuredSchema
              ? "bg-violet-50 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300"
              : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
          }`}>
            {configuredSchema
              ? target.kind === "orientation" && target.autoReview
                ? "Adaptive contract"
                : "Workflow contract"
              : target.kind === "orientation"
                ? "Pipeline default"
                : "Managed Markdown"}
          </span>
          {target.kind === "step" && !target.step.enabled && (
            <span className="rounded-full bg-gray-100 px-2 py-0.5 text-[10px] text-gray-500 dark:bg-gray-800 dark:text-gray-400">
              Step disabled
            </span>
          )}
          {target.kind === "step" && target.publishesFindings && (
            <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300">
              Published findings
            </span>
          )}
        </div>
        <p className="mt-1.5 max-w-3xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
          {isManagedText
            ? "Pipeline constrains the provider response with this envelope, then stores only its content field as the step’s Markdown report."
            : isDefaultOrientation
              ? "Pipeline requires one JSON object. Add a custom schema when the workflow depends on particular orientation fields."
              : catalogBacked
                ? "Pipeline stores this compact contract, resolves its live catalog references into strict enums for the provider call, and validates the result against the same catalog snapshot."
                : "Pipeline sends this contract through every provider’s native structured-output channel and validates the returned artifact again before saving it."}
        </p>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        {isManagedText || isDefaultOrientation ? (
          <div className="max-w-3xl space-y-5">
            <div>
              <div className="mb-2 flex items-center justify-between gap-3">
                <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
                  Effective provider schema
                </h3>
                <span className="text-[10px] text-gray-500 dark:text-gray-400">Managed by Pipeline</span>
              </div>
              <pre
                aria-label={target.kind === "orientation"
                  ? "Effective orientation schema"
                  : `Effective schema for ${target.step.label}`}
                className="overflow-x-auto rounded-xl border border-gray-200 bg-gray-50 p-4 font-mono text-xs leading-relaxed text-gray-800 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200"
              >
                {schemaText(isManagedText ? TEXT_ARTIFACT_SCHEMA : DEFAULT_ORIENTATION_SCHEMA)}
              </pre>
            </div>

            <div className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
              <h3 className="text-sm font-medium text-gray-800 dark:text-gray-200">
                {isManagedText ? "Need structured JSON instead?" : "Need a more specific survey contract?"}
              </h3>
              <p className="mt-1 text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                {isManagedText
                  ? "Switching to a custom schema makes the durable step result JSON. Supporting files remain separate artifacts."
                  : "A custom schema can require the exact fields that downstream conditions and prompts rely on."}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() => applyConfiguredSchema(
                    isManagedText ? STARTER_JSON_SCHEMA : DEFAULT_ORIENTATION_SCHEMA,
                  )}
                  className="rounded-lg bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-white"
                >
                  {isManagedText ? "Create custom JSON schema" : "Customize schema"}
                </button>
                {isManagedText && (
                  <button
                    type="button"
                    onClick={() => applyConfiguredSchema(ISSUES_SCHEMA)}
                    className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800"
                  >
                    Use issues schema
                  </button>
                )}
              </div>
            </div>
          </div>
        ) : (
          <div className="max-w-4xl space-y-4">
            {catalogBacked && (
              <section className="rounded-xl border border-violet-200 bg-violet-50/60 p-4 dark:border-violet-900 dark:bg-violet-950/20">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="text-sm font-medium text-violet-950 dark:text-violet-100">
                      Live specialist catalog
                    </h3>
                    <p className="mt-1 max-w-2xl text-xs leading-relaxed text-violet-800/80 dark:text-violet-200/80">
                      Catalog references keep the saved workflow small. Pipeline expands them into ordinary enums only for the current provider call.
                    </p>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    {onBrowseCatalog && (
                      <button
                        type="button"
                        onClick={() => onBrowseCatalog("subjects")}
                        className="rounded-lg border border-violet-300 bg-white px-3 py-1.5 text-xs font-medium text-violet-800 hover:bg-violet-50 dark:border-violet-800 dark:bg-violet-950/50 dark:text-violet-200"
                      >
                        Browse catalog
                      </button>
                    )}
                    <button
                      type="button"
                      disabled={resolving || !!error}
                      onClick={toggleResolvedSchema}
                      className="rounded-lg bg-violet-700 px-3 py-1.5 text-xs font-medium text-white hover:bg-violet-800 disabled:cursor-not-allowed disabled:opacity-50"
                    >
                      {resolving
                        ? "Resolving…"
                        : resolvedSchema
                          ? "Hide resolved schema"
                          : "View resolved provider schema"}
                    </button>
                  </div>
                </div>
                {catalog && (
                  <div className="mt-3 flex flex-wrap gap-2 text-[11px] text-violet-800 dark:text-violet-200">
                    <span className="rounded-full bg-white px-2.5 py-1 dark:bg-violet-950/60">
                      {catalog.subjectCount} subject roles
                    </span>
                    <span className="rounded-full bg-white px-2.5 py-1 dark:bg-violet-950/60">
                      {catalog.methodCount} method roles
                    </span>
                    <span className="rounded-full bg-white px-2.5 py-1 dark:bg-violet-950/60">
                      {catalog.genreCount} document genres
                    </span>
                    <span className="rounded-full bg-white px-2.5 py-1 font-mono text-[10px] dark:bg-violet-950/60">
                      {catalog.revision.slice(0, 19)}…
                    </span>
                  </div>
                )}
                {resolutionError && (
                  <p role="alert" className="mt-3 text-xs text-red-700 dark:text-red-300">
                    Could not resolve catalog references: {resolutionError}
                  </p>
                )}
              </section>
            )}
            <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
              <div>
                <label
                  htmlFor="artifact-schema-editor"
                  className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400"
                >
                  Portable JSON Schema
                </label>
                <p className="mt-1 text-[11px] text-gray-500 dark:text-gray-400">
                  Invalid drafts stay local and cannot be saved into the workflow.
                </p>
              </div>
              <div className="flex flex-wrap gap-3">
                {target.kind === "step" && (
                  <button
                    type="button"
                    onClick={() => applyConfiguredSchema(ISSUES_SCHEMA)}
                    className="text-xs font-medium text-gray-600 underline underline-offset-2 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
                  >
                    Use issues schema
                  </button>
                )}
                {!(target.kind === "orientation" && target.autoReview) && (
                  <button
                    type="button"
                    onClick={() => applyConfiguredSchema(null)}
                    className="text-xs font-medium text-gray-600 underline underline-offset-2 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
                  >
                    {target.kind === "orientation" ? "Use Pipeline default" : "Return to Markdown output"}
                  </button>
                )}
              </div>
            </div>
            <textarea
              id="artifact-schema-editor"
              aria-label={target.kind === "orientation"
                ? "Orientation output JSON schema"
                : `Output JSON schema for ${target.step.label}`}
              value={draft}
              onChange={(event) => changeDraft(event.target.value)}
              spellCheck={false}
              className="min-h-[26rem] w-full resize-y rounded-xl border border-gray-300 bg-white p-4 font-mono text-xs leading-relaxed text-gray-900 focus:border-transparent focus:outline-none focus:ring-2 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100"
            />
            {error ? (
              <p role="alert" className="mt-2 text-xs text-red-600 dark:text-red-400">
                Invalid schema: {error}
              </p>
            ) : (
              <p className="mt-2 text-xs text-green-700 dark:text-green-400">
                Valid portable Pipeline artifact schema.
              </p>
            )}
            {resolvedSchema && (
              <section className="rounded-xl border border-gray-200 bg-gray-50 p-4 dark:border-gray-700 dark:bg-gray-900">
                <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
                    Resolved provider schema
                  </h3>
                  <span className="text-[10px] text-gray-500 dark:text-gray-400">
                    Generated for the current catalog
                  </span>
                </div>
                <pre
                  aria-label="Resolved orientation provider schema"
                  className="max-h-[32rem] overflow-auto rounded-lg bg-white p-3 font-mono text-xs leading-relaxed text-gray-800 dark:bg-gray-950 dark:text-gray-200"
                >
                  {schemaText(resolvedSchema)}
                </pre>
              </section>
            )}
            {target.kind === "step" && target.publishesFindings && (
              <p className="mt-3 rounded-lg bg-amber-50 px-3 py-2 text-xs leading-relaxed text-amber-800 dark:bg-amber-950/30 dark:text-amber-200">
                This contract powers the published findings product. Returning to Markdown also turns off findings publication for this step.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
