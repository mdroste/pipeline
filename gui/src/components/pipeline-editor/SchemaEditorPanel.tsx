import { workflowSchemaClient } from "../../lib/workflowSchemaClient";
import type { ProviderSchemaPreview } from "../../lib/workflowSchemaClient";
import { useEffect, useMemo, useRef, useState } from "react";
import type { AutoReviewCatalog, StepConfig } from "../../lib/types";
import { AUTO_REVIEW_CONTRACT } from "../../lib/autoReview";
import { ISSUES_SCHEMA } from "./stepTemplates";
import { exampleInstance, outputSchemaError } from "./utils";
import SchemaTreeView from "./SchemaTreeView";

const DIALECT_KEYWORDS: [string, string][] = [
  [
    "type",
    "object, array, string, number, integer, boolean, or null; the artifact root must be object",
  ],
  ["properties / required", "object fields; required lists the mandatory ones"],
  ["items", "the element schema of an array"],
  ["enum", "closed set of allowed values"],
  [
    "minItems / maxItems / uniqueItems",
    "array bounds; uniqueItems is enforced by Pipeline, not providers",
  ],
  ["minLength", "minimum string length; enforced by Pipeline, not providers"],
  ["title / description", "documentation shown here and sent to providers"],
  [
    "x-pipeline-schema",
    "root-only reference to a live host contract (findings-v1, findings-v2, or findings-v2-validation)",
  ],
  [
    "x-pipeline-findings-version",
    "root-only: the findings contract version, a non-negative integer",
  ],
  [
    "x-pipeline-findings-taxonomy",
    "root-only: the categories findings may use, as unique non-empty strings",
  ],
  [
    "x-pipeline-validation-ledger",
    "root-only: marks a contract that records a decision for every finding",
  ],
  [
    "x-pipeline-preserve-findings-from",
    "root-only: the response must keep the named step's finding ids, in order",
  ],
];

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
  onStepSchemaChange: (
    id: string,
    schema: Record<string, unknown> | null,
  ) => void;
  onDraftValidityChange: (valid: boolean) => void;
  onBrowseCatalog?: (tab: "subjects" | "methods") => void;
}) {
  const targetKey =
    target.kind === "orientation" ? "orientation" : target.step.id;
  const draftKey = `${scopeKey}:${targetKey}`;
  const configuredSchema =
    target.kind === "orientation"
      ? (target.schema ?? null)
      : (target.step.output_schema ?? null);
  const [draft, setDraft] = useState(
    configuredSchema ? schemaText(configuredSchema) : "",
  );
  const [error, setError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<AutoReviewCatalog | null>(null);
  const [resolvedSchema, setResolvedSchema] = useState<Record<
    string,
    unknown
  > | null>(null);
  const [resolutionError, setResolutionError] = useState<string | null>(null);
  const [resolving, setResolving] = useState(false);
  const [providerPreview, setProviderPreview] =
    useState<ProviderSchemaPreview | null>(null);
  const [providerPreviewError, setProviderPreviewError] = useState<
    string | null
  >(null);
  const [previewingProvider, setPreviewingProvider] = useState(false);
  const [referencedSchema, setReferencedSchema] = useState<Record<
    string,
    unknown
  > | null>(null);

  // The schema object this panel last sent to the workflow, or loaded from it.
  // The owner stores that same object, so identity tells this panel's own
  // edits apart from outside changes such as undo, redo or a reset.
  const syncedSchema = useRef<{
    key: string;
    schema: Record<string, unknown> | null;
  } | null>(null);
  // Bumped whenever the draft changes, so a preview or resolution still in
  // flight cannot land on a later draft or another step.
  const draftGeneration = useRef(0);
  const clearDerivedSchemas = () => {
    draftGeneration.current += 1;
    setResolvedSchema(null);
    setResolutionError(null);
    setResolving(false);
    setProviderPreview(null);
    setProviderPreviewError(null);
    setPreviewingProvider(false);
  };
  const emitSchema = (schema: Record<string, unknown> | null) => {
    syncedSchema.current = { key: draftKey, schema };
    if (target.kind === "orientation") {
      onOrientationSchemaChange(schema);
    } else {
      onStepSchemaChange(target.step.id, schema);
    }
  };

  useEffect(() => {
    const synced = syncedSchema.current;
    if (synced?.key === draftKey && synced.schema === configuredSchema) return;
    // A new target, or an outside change to this one, replaces the draft.
    // The panel's own valid edits keep their local formatting and caret.
    syncedSchema.current = { key: draftKey, schema: configuredSchema };
    setDraft(configuredSchema ? schemaText(configuredSchema) : "");
    setError(null);
    clearDerivedSchemas();
    onDraftValidityChange(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draftKey, configuredSchema]);

  // The current draft as a valid parsed schema, for the structure and example
  // views. An invalid draft hides the example; the textarea stays.
  const parsedDraft = useMemo<Record<string, unknown> | null>(() => {
    if (!draft.trim()) return null;
    try {
      const parsed = JSON.parse(draft) as Record<string, unknown>;
      return outputSchemaError(parsed) ? null : parsed;
    } catch {
      return null;
    }
  }, [draft]);

  const liveReference =
    typeof parsedDraft?.["x-pipeline-schema"] === "string"
      ? (parsedDraft["x-pipeline-schema"] as string)
      : null;

  // A live reference stands in for the whole contract; the backend resolves
  // it, with any taxonomy beside it, so the structure view shows what the step
  // actually promises for every reference it knows.
  useEffect(() => {
    if (!liveReference || !parsedDraft) {
      setReferencedSchema(null);
      return;
    }
    let live = true;
    workflowSchemaClient
      .preview({ schema: parsedDraft })
      .then((preview) => {
        if (live) setReferencedSchema(preview.projected);
      })
      .catch(() => {
        if (live) setReferencedSchema(null);
      });
    return () => {
      live = false;
    };
  }, [liveReference, parsedDraft]);

  const structureSchema = liveReference ? referencedSchema : parsedDraft;

  // While a draft is momentarily invalid, keep showing the last valid
  // structure so the view above the textarea does not collapse mid-typing.
  const [lastStructure, setLastStructure] = useState<{
    key: string;
    schema: Record<string, unknown>;
  } | null>(null);
  useEffect(() => {
    if (structureSchema)
      setLastStructure({ key: draftKey, schema: structureSchema });
  }, [draftKey, structureSchema]);
  const shownStructure =
    structureSchema ??
    (lastStructure?.key === draftKey ? lastStructure.schema : null);

  useEffect(() => () => onDraftValidityChange(true), [onDraftValidityChange]);

  const catalogBacked =
    target.kind === "orientation" && target.autoReview && !!configuredSchema;

  useEffect(() => {
    if (!catalogBacked) {
      setCatalog(null);
      return;
    }
    let live = true;
    workflowSchemaClient
      .catalog()
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
    clearDerivedSchemas();
    onDraftValidityChange(true);
    emitSchema(schema);
  };

  // The canonical findings contract is served from the binary so the inserted
  // template cannot drift from host validation and the published-findings
  // machinery.
  const insertFindingsSchema = async () => {
    try {
      const schema = await workflowSchemaClient.findings();
      applyConfiguredSchema(schema);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      onDraftValidityChange(false);
    }
  };

  const changeDraft = (next: string) => {
    setDraft(next);
    clearDerivedSchemas();
    if (!next.trim()) {
      // Clearing the box is usually the first half of pasting a new schema.
      // Removing the schema takes the explicit button below.
      setError(
        target.kind === "step"
          ? "Paste a schema or choose Return to Markdown output."
          : target.autoReview
            ? "This review requires its generated orientation schema."
            : "Paste a schema or choose Use Pipeline default.",
      );
      onDraftValidityChange(false);
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
      if (
        target.kind === "step" &&
        (parsed["x-pipeline-contract"] !== undefined ||
          parsed["x-pipeline-adaptive-agent-count"] !== undefined)
      ) {
        setError(
          "x-pipeline contract settings are reserved for the orientation schema.",
        );
        onDraftValidityChange(false);
        return;
      }
      if (
        target.kind === "orientation" &&
        target.autoReview &&
        parsed["x-pipeline-contract"] !== AUTO_REVIEW_CONTRACT
      ) {
        setError(
          `This review must retain x-pipeline-contract: "${AUTO_REVIEW_CONTRACT}". ` +
            "Create a separate custom review to replace this host-owned contract.",
        );
        onDraftValidityChange(false);
        return;
      }
      setError(null);
      onDraftValidityChange(true);
      emitSchema(parsed);
    } catch (parseError) {
      setError(
        parseError instanceof Error ? parseError.message : "invalid JSON",
      );
      onDraftValidityChange(false);
    }
  };

  const toggleProviderPreview = async () => {
    if (providerPreview) {
      setProviderPreview(null);
      return;
    }
    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(draft) as Record<string, unknown>;
    } catch {
      setProviderPreviewError(
        "Fix the JSON draft before previewing the provider schema.",
      );
      return;
    }
    const generation = draftGeneration.current;
    setPreviewingProvider(true);
    setProviderPreviewError(null);
    try {
      const preview = await workflowSchemaClient.preview({
        schema: parsed,
      });
      if (draftGeneration.current === generation) setProviderPreview(preview);
    } catch (caught) {
      if (draftGeneration.current === generation) {
        setProviderPreviewError(
          caught instanceof Error ? caught.message : String(caught),
        );
      }
    } finally {
      if (draftGeneration.current === generation) setPreviewingProvider(false);
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
      setResolutionError(
        "Fix the JSON draft before resolving its catalog references.",
      );
      return;
    }
    const generation = draftGeneration.current;
    setResolving(true);
    setResolutionError(null);
    try {
      const resolved = await workflowSchemaClient.resolve({ schema: parsed });
      if (draftGeneration.current === generation) setResolvedSchema(resolved);
    } catch (caught) {
      if (draftGeneration.current === generation) {
        setResolutionError(
          caught instanceof Error ? caught.message : String(caught),
        );
      }
    } finally {
      if (draftGeneration.current === generation) setResolving(false);
    }
  };

  const title =
    target.kind === "orientation"
      ? "Orientation map schema"
      : `${target.step.label} output schema`;
  const isManagedText = target.kind === "step" && !configuredSchema;
  const isDefaultOrientation =
    target.kind === "orientation" && !configuredSchema;

  if (target.kind === "step" && target.step.kind === "analyzer") {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="border-b border-gray-200 px-5 py-4 dark:border-gray-700">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
              {target.step.label} result
            </h2>
            <span className="rounded-full bg-gray-100 px-2 py-0.5 text-[10px] font-medium text-gray-600 dark:bg-gray-800 dark:text-gray-300">
              Pipeline writes this result
            </span>
          </div>
          <p className="mt-1.5 max-w-3xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
            This check makes no model call. Pipeline writes its result in a
            fixed format, so there is no format to choose here.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="border-b border-gray-200 px-5 py-4 dark:border-gray-700">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-base font-semibold text-gray-900 dark:text-gray-100">
            {title}
          </h2>
          <span
            className={`rounded-full px-2 py-0.5 text-[10px] font-medium ${
              configuredSchema
                ? "bg-violet-50 text-violet-700 dark:bg-violet-950/50 dark:text-violet-300"
                : "bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300"
            }`}
          >
            {configuredSchema
              ? target.kind === "orientation" && target.autoReview
                ? "Adaptive contract"
                : "Review contract"
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
            <span className="rounded-full bg-emerald-50 px-2 py-0.5 text-[10px] text-emerald-700 dark:bg-gray-800 dark:text-gray-300">
              Published findings
            </span>
          )}
        </div>
        <p className="mt-1.5 max-w-3xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
          {isManagedText
            ? "Pipeline constrains the provider response with this envelope, then stores only its content field as the step’s Markdown report."
            : isDefaultOrientation
              ? "Pipeline requires one JSON object. Add a custom schema when the review depends on particular orientation fields."
              : catalogBacked
                ? "Pipeline stores this compact contract, resolves its live catalog references into strict enums for the provider call, and validates the result against the same catalog snapshot."
                : "Pipeline asks the provider to return this JSON format and checks the response before saving it."}
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
                <span className="text-[10px] text-gray-500 dark:text-gray-400">
                  Managed by Pipeline
                </span>
              </div>
              <pre
                aria-label={
                  target.kind === "orientation"
                    ? "Effective orientation schema"
                    : `Effective schema for ${target.step.label}`
                }
                className="overflow-x-auto rounded-xl border border-gray-200 bg-gray-50 p-4 font-mono text-xs leading-relaxed text-gray-800 dark:border-gray-700 dark:bg-gray-900 dark:text-gray-200"
              >
                {schemaText(
                  isManagedText
                    ? TEXT_ARTIFACT_SCHEMA
                    : DEFAULT_ORIENTATION_SCHEMA,
                )}
              </pre>
            </div>

            <div className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
              <h3 className="text-sm font-medium text-gray-800 dark:text-gray-200">
                {isManagedText
                  ? "Need structured JSON instead?"
                  : "Need a more specific survey contract?"}
              </h3>
              <p className="mt-1 text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                {isManagedText
                  ? "A custom schema saves the step’s result as JSON. Supporting files are saved separately."
                  : "A custom schema can require the exact fields that downstream conditions and prompts rely on."}
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <button
                  type="button"
                  onClick={() =>
                    applyConfiguredSchema(
                      isManagedText
                        ? STARTER_JSON_SCHEMA
                        : DEFAULT_ORIENTATION_SCHEMA,
                    )
                  }
                  className="rounded-lg bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-gray-800 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-white"
                >
                  {isManagedText
                    ? "Create custom JSON schema"
                    : "Customize schema"}
                </button>
                {isManagedText && (
                  <>
                    <button
                      type="button"
                      onClick={() => void insertFindingsSchema()}
                      className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      Use findings schema
                    </button>
                    <button
                      type="button"
                      onClick={() => applyConfiguredSchema(ISSUES_SCHEMA)}
                      className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800"
                    >
                      Use legacy issues schema
                    </button>
                  </>
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
                      Catalog references keep the saved review small. Pipeline
                      expands them into ordinary enums only for the current
                      provider call.
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
                  <p
                    role="alert"
                    className="mt-3 text-xs text-red-700 dark:text-red-300"
                  >
                    Could not resolve catalog references: {resolutionError}
                  </p>
                )}
              </section>
            )}
            {liveReference && (
              <section className="rounded-xl border border-emerald-200 bg-emerald-50/60 p-4 dark:border-gray-700 dark:bg-gray-800/40">
                <h3 className="text-sm font-medium text-emerald-950 dark:text-gray-100">
                  Live contract reference:{" "}
                  <span className="font-mono">{liveReference}</span>
                </h3>
                <p className="mt-1 max-w-2xl text-xs leading-relaxed text-emerald-800/80 dark:text-gray-300">
                  This step uses Pipeline’s standard output format, updated for
                  each request. The current format appears below. Replace the
                  reference with a full schema to customize it.
                </p>
              </section>
            )}
            {shownStructure && (
              <div>
                <div className="mb-2 flex flex-wrap items-baseline gap-x-2">
                  <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
                    Structure
                  </h3>
                  {!structureSchema && (
                    <span className="text-[11px] text-gray-500 dark:text-gray-400">
                      Showing the last valid version
                    </span>
                  )}
                </div>
                <SchemaTreeView schema={shownStructure} />
              </div>
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
                  Invalid drafts stay local and cannot be saved into the review.
                </p>
              </div>
              <div className="flex flex-wrap gap-3">
                {target.kind === "step" && (
                  <button
                    type="button"
                    onClick={() => void insertFindingsSchema()}
                    className="text-xs font-medium text-gray-600 underline underline-offset-2 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
                  >
                    Use findings schema
                  </button>
                )}
                {target.kind === "step" && (
                  <button
                    type="button"
                    onClick={() => applyConfiguredSchema(ISSUES_SCHEMA)}
                    className="text-xs font-medium text-gray-600 underline underline-offset-2 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
                  >
                    Use legacy issues schema
                  </button>
                )}
                {!(target.kind === "orientation" && target.autoReview) && (
                  <button
                    type="button"
                    onClick={() => applyConfiguredSchema(null)}
                    className="text-xs font-medium text-gray-600 underline underline-offset-2 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
                  >
                    {target.kind === "orientation"
                      ? "Use Pipeline default"
                      : "Return to Markdown output"}
                  </button>
                )}
              </div>
            </div>
            <textarea
              id="artifact-schema-editor"
              aria-label={
                target.kind === "orientation"
                  ? "Orientation output JSON schema"
                  : `Output JSON schema for ${target.step.label}`
              }
              value={draft}
              onChange={(event) => changeDraft(event.target.value)}
              spellCheck={false}
              className="min-h-[26rem] w-full resize-y rounded-xl border border-gray-300 bg-white p-4 font-mono text-xs leading-relaxed text-gray-900 focus:border-transparent focus:outline-none focus:ring-2 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100"
            />
            {error ? (
              <p
                role="alert"
                className="mt-2 text-xs text-red-600 dark:text-red-400"
              >
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
                This format lets Pipeline save individual findings. Switching to
                Markdown stops this step from adding findings to the Issues
                list.
              </p>
            )}

            <section className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <h3 className="text-sm font-medium text-gray-800 dark:text-gray-200">
                    Provider view
                  </h3>
                  <p className="mt-1 max-w-2xl text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                    What providers actually receive: references and catalogs
                    resolved, host-only keywords removed.
                  </p>
                </div>
                <button
                  type="button"
                  disabled={previewingProvider || !!error}
                  onClick={() => void toggleProviderPreview()}
                  className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 hover:bg-gray-50 disabled:cursor-not-allowed disabled:opacity-50 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-800"
                >
                  {previewingProvider
                    ? "Previewing…"
                    : providerPreview
                      ? "Hide provider view"
                      : "View provider schema"}
                </button>
              </div>
              {providerPreviewError && (
                <p
                  role="alert"
                  className="mt-3 text-xs text-red-700 dark:text-red-300"
                >
                  Could not build the provider schema: {providerPreviewError}
                </p>
              )}
              {providerPreview && (
                <div className="mt-3 space-y-3">
                  <div className="flex flex-wrap items-center gap-2 text-[11px]">
                    <span
                      className={`rounded-full px-2.5 py-1 font-medium ${
                        providerPreview.bytes <= providerPreview.limit
                          ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/40 dark:text-emerald-300"
                          : "bg-red-50 text-red-700 dark:bg-red-950/40 dark:text-red-300"
                      }`}
                    >
                      {(providerPreview.bytes / 1024).toFixed(1)} KiB of{" "}
                      {Math.round(providerPreview.limit / 1024)} KiB transport
                      budget
                    </span>
                    <span className="rounded-full bg-gray-100 px-2.5 py-1 text-gray-700 dark:bg-gray-800 dark:text-gray-300">
                      Anthropic / Gemini: native constraint
                    </span>
                    <span
                      className={`rounded-full px-2.5 py-1 ${
                        providerPreview.openaiStrict
                          ? "bg-gray-100 text-gray-700 dark:bg-gray-800 dark:text-gray-300"
                          : "bg-amber-50 text-amber-800 dark:bg-amber-950/40 dark:text-amber-200"
                      }`}
                      title={
                        providerPreview.openaiStrict
                          ? "Every declared property is required, so OpenAI decodes against the exact schema."
                          : "Optional properties keep OpenAI in advisory json_schema mode; Pipeline still validates the returned artifact."
                      }
                    >
                      {providerPreview.openaiStrict
                        ? "OpenAI: strict decoding"
                        : "OpenAI: advisory + host validation"}
                    </span>
                  </div>
                  <pre
                    aria-label="Provider schema preview"
                    className="max-h-[28rem] overflow-auto rounded-lg bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800 dark:bg-gray-950 dark:text-gray-200"
                  >
                    {schemaText(providerPreview.projected)}
                  </pre>
                </div>
              )}
            </section>

            {structureSchema && (
              <details className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
                <summary className="cursor-pointer text-sm font-medium text-gray-800 dark:text-gray-200">
                  Example response
                </summary>
                <p className="mt-1 text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                  An example of the required format. The model supplies the
                  content.
                </p>
                <pre className="mt-2 max-h-[20rem] overflow-auto rounded-lg bg-gray-50 p-3 font-mono text-xs leading-relaxed text-gray-800 dark:bg-gray-950 dark:text-gray-200">
                  {JSON.stringify(exampleInstance(structureSchema), null, 2)}
                </pre>
              </details>
            )}

            <details className="rounded-xl border border-gray-200 bg-white p-4 dark:border-gray-700 dark:bg-gray-900">
              <summary className="cursor-pointer text-sm font-medium text-gray-800 dark:text-gray-200">
                Supported schema keywords
              </summary>
              <p className="mt-1 text-xs leading-relaxed text-gray-500 dark:text-gray-400">
                Pipeline supports only schema rules it can check across all
                providers.
              </p>
              <dl className="mt-2 space-y-1.5">
                {DIALECT_KEYWORDS.map(([keyword, note]) => (
                  <div key={keyword} className="flex flex-wrap gap-x-2 text-xs">
                    <dt className="font-mono font-medium text-gray-800 dark:text-gray-200">
                      {keyword}
                    </dt>
                    <dd className="text-gray-500 dark:text-gray-400">{note}</dd>
                  </div>
                ))}
              </dl>
            </details>
          </div>
        )}
      </div>
    </div>
  );
}
