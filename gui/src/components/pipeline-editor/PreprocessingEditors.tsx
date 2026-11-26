import { memo, useEffect, useRef } from "react";
import { workflowSchemaClient } from "../../lib/workflowSchemaClient";
import { notify } from "../DialogService";
import type { ExtractionConfig } from "../../lib/workflowTypes";
import { ADAPTIVE_AGENT_COUNT_KEY } from "../../lib/autoReview";
import PromptEditor from "../PromptEditor";
import { MemoizedExtraInputsEditor as ExtraInputsEditor } from "./ProfileSettingsEditors";

const EXTRACTION_METHODS: { value: string; label: string; hint: string }[] = [
  {
    value: "",
    label: "Inherit from global Settings",
    hint: "Use whatever PDF extractor is configured globally.",
  },
  {
    value: "auto",
    label: "Auto",
    hint: "Try the global setting; same as inherit.",
  },
  {
    value: "llm",
    label: "LLM",
    hint: "Your selected model transcribes the PDF a few pages at a time, with checks for missing text. Slower, but retains equations and original typos.",
  },
  {
    value: "paddleocr-vl-full",
    label: "Local engine: PaddleOCR-VL 1.6 Full Parser",
    hint: "Read the PDF locally, preserving reading order, headings, equations, and tables that span pages. Uses Pipeline’s installed parser.",
  },
  {
    value: "pdftotext",
    label: "pdftotext (basic)",
    hint: "Fast, but equations are lost. Uses bundled poppler.",
  },
];

function ExtractionEditor({
  extraction,
  onChange,
}: {
  extraction: ExtractionConfig;
  onChange: (patch: Partial<ExtractionConfig>) => void;
}) {
  const method = extraction.method ?? "";
  const hint = EXTRACTION_METHODS.find((m) => m.value === method)?.hint;
  const inputMode = extraction.input_mode || "document";

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
      <div className="p-4 space-y-5">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">
            Input & PDF Extraction
          </h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            Choose the input type and how Pipeline reads its text before the
            review starts.
          </p>
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Input mode (per review)
          </label>
          <select
            aria-label="Review input mode"
            value={inputMode}
            onChange={(e) => onChange({ input_mode: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            <option value="document">
              Document — a PDF, LaTeX, Word, or PowerPoint file
            </option>
            <option value="folder">
              Folder — inventory a directory; steps Read files on demand
            </option>
            <option value="none">None — run from the step prompts alone</option>
          </select>
          <p className="text-[0.6875rem] text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">
            Selecting a folder in the main window uses folder mode
            automatically, whatever this is set to.
          </p>
        </div>

        {/* Extra named inputs */}
        <ExtraInputsEditor
          slots={extraction.extra_inputs ?? []}
          onChange={(extra_inputs) => onChange({ extra_inputs })}
        />

        {inputMode !== "document" ? (
          <div className="text-[0.6875rem] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
            Extraction settings below apply only to document inputs.
          </div>
        ) : null}

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Method (per review)
          </label>
          <select
            aria-label="PDF extraction method"
            value={method}
            onChange={(e) => onChange({ method: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            {EXTRACTION_METHODS.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
          {hint && (
            <p className="text-[0.6875rem] mt-1.5 leading-relaxed text-gray-500 dark:text-gray-400">
              {hint}
            </p>
          )}
        </div>

        <div className="text-[0.6875rem] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
          Pipeline uses the PDF method you choose and stops if it cannot read
          the document completely. Adjust speed, memory, OCR, and image options
          in Settings → Reviews → PDF extraction. LaTeX files are read directly.
        </div>
      </div>
    </div>
  );
}

function OrientationEditor({
  prompt,
  schema,
  inputMode,
  autoReview = false,
  profileId,
  onPromptChange,
  onSchemaChange,
}: {
  prompt: string;
  schema?: Record<string, unknown> | null;
  inputMode: string;
  autoReview?: boolean;
  /** Selects a built-in's own stock router when restoring defaults. */
  profileId?: string;
  onPromptChange: (p: string) => void;
  onSchemaChange: (schema: Record<string, unknown> | null) => void;
}) {
  const resetRequest = useRef(0);
  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
      resetRequest.current += 1;
    },
    [],
  );

  const insertDefault = async (name: string) => {
    const request = ++resetRequest.current;
    try {
      const defaults = await workflowSchemaClient.orientationDefaults({ name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange(defaults.prompt);
        onSchemaChange(defaults.schema);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        notify(
          `Failed to load the default document preparation prompt: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  const useDefault = async () => {
    const request = ++resetRequest.current;
    const name = inputMode === "folder" ? "orientation_folder" : "orientation";
    try {
      const defaults = await workflowSchemaClient.orientationDefaults({ name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange("");
        onSchemaChange(defaults.schema);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        notify(
          `Failed to load the default document overview schema: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  const changePrompt = (next: string) => {
    resetRequest.current += 1;
    onPromptChange(next);
  };

  const restoreRouterPrompt = async () => {
    const request = ++resetRequest.current;
    try {
      // The router prompt lists the live catalog's subjects, methods, and
      // genres; the saved schema must match it or newer routing picks fail
      // validation. Restore both together, keeping only the configured
      // adaptive-agent count from the current schema. Built-ins restore
      // their own pair (Fast's method cap, Slide Review's genre pin).
      const defaults = await workflowSchemaClient.autoOrientationDefaults({
        profileId,
      });
      if (mounted.current && request === resetRequest.current) {
        const nextSchema = { ...defaults.schema };
        const count = schema?.[ADAPTIVE_AGENT_COUNT_KEY];
        if (count !== undefined) {
          nextSchema[ADAPTIVE_AGENT_COUNT_KEY] = count;
        }
        onPromptChange(defaults.prompt);
        onSchemaChange(nextSchema);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        notify(
          `Failed to load the adaptive router prompt and schema: ${
            error instanceof Error ? error.message : String(error)
          }`,
        );
      }
    }
  };

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="p-4 border-b border-gray-200 dark:border-gray-700 space-y-3">
        <div>
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">
            Document bundle preparation
          </h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            {autoReview ? (
              <>
                Prepares the document's text and structure, then chooses subject
                and method reviewers. The planner starts with relevant
                candidates and can look up more reviewers or supporting passages
                together. The document overview and review plan are saved
                separately.
              </>
            ) : (
              <>
                Prepares a document overview before the review starts. Steps
                that select the overview receive it through {"{orientation}"}.
                The prompt and output schema determine what it contains.
              </>
            )}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <span className="rounded bg-success-soft px-1.5 py-0.5 text-[0.625rem] font-medium text-success dark:bg-gray-800 dark:text-gray-300">
            Required
          </span>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            Runs before every review
          </span>
          <span className="ml-auto rounded-full bg-violet-50 px-2 py-0.5 text-[0.625rem] font-medium text-violet-700 dark:bg-violet-950/40 dark:text-violet-300">
            {schema ? "Schema configured" : "Object-only schema"} · Schemas
          </span>
        </div>
      </div>

      <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between">
        <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Prompt
        </label>
        <div className="flex items-center gap-3">
          {autoReview ? (
            // A plain survey prompt (or an empty override, which falls back
            // to the stock paper survey) cannot produce the validated
            // review_plan this workflow's schema requires — every run would
            // fail orientation. Offer only the generated router prompt.
            <button
              type="button"
              onClick={() => void restoreRouterPrompt()}
              className="text-[0.625rem] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
              title="Restore the generated document preparation and review planning prompt and its matching output schema. A configured adaptive-agent count is kept."
            >
              Restore adaptive router prompt & schema
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => void insertDefault("orientation_generic")}
                className="text-[0.625rem] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the generic survey prompt (works for any input)."
              >
                Insert generic survey
              </button>
              <button
                type="button"
                onClick={() => void insertDefault("orientation_folder")}
                className="text-[0.625rem] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the folder survey prompt (explores the folder with the Read tool)."
              >
                Insert folder survey
              </button>
              <button
                type="button"
                onClick={() => void insertDefault("orientation")}
                className="text-[0.625rem] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the paper-review survey prompt (sections, theorems, tables, notation)."
              >
                Insert paper survey
              </button>
              <button
                type="button"
                onClick={() => void useDefault()}
                disabled={prompt.trim() === ""}
                className="text-[0.625rem] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100
                           disabled:opacity-40 disabled:hover:text-gray-600 dark:disabled:hover:text-gray-400 transition-colors"
                title="Clear the override; the default template will be used."
              >
                Use default
              </button>
            </>
          )}
        </div>
      </div>
      <div className="flex-1 min-h-0 p-4 flex flex-col">
        <div className="flex-1 min-h-0">
          <PromptEditor
            value={prompt}
            onChange={changePrompt}
            context={{ kind: "orientation", autoReview }}
            ariaLabel="Document overview prompt"
          />
        </div>
        {autoReview && (
          <p className="text-[0.6875rem] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Pipeline fills the catalog placeholders with the current specialist
            list when the review starts. Browse the Catalog tabs to see those
            specialists.
          </p>
        )}
        {prompt.trim() === "" && (
          <p className="text-[0.6875rem] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Empty — using the bundled default:{" "}
            <span className="font-mono">prompts/orientation.md</span> for
            document inputs,{" "}
            <span className="font-mono">prompts/orientation_folder.md</span> for
            folder inputs (overridable at{" "}
            <span className="font-mono">~/.pipeline/prompts/</span>). Stock
            prompts adapt to the input mode; a customized prompt is used as-is.
          </p>
        )}
      </div>
    </div>
  );
}

// --- Step row ---

export const MemoizedExtractionEditor = memo(ExtractionEditor);
export const MemoizedOrientationEditor = memo(OrientationEditor);
