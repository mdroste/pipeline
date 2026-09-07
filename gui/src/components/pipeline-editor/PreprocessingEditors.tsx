import { memo, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { notify } from "../DialogService";
import type { ExtractionConfig } from "../../lib/types";
import { ADAPTIVE_AGENT_COUNT_KEY } from "../../lib/autoReview";
import PromptEditor from "../PromptEditor";
import { MemoizedExtraInputsEditor as ExtraInputsEditor } from "./ProfileSettingsEditors";

const EXTRACTION_METHODS: { value: string; label: string; hint: string }[] = [
  { value: "", label: "Inherit from global Settings", hint: "Use whatever PDF extractor is configured globally." },
  { value: "auto", label: "Auto", hint: "Try the global setting; same as inherit." },
  { value: "llm", label: "LLM", hint: "Bounded, page-verified transcription through the active provider. Slower, but preserves equations and original typos." },
  { value: "paddleocr-vl-full", label: "Local engine: PaddleOCR-VL 1.6 Full Parser", hint: "Official layout-aware client with structured regions, title hierarchy, formula metadata, and cross-page table reconstruction. Reuses Pipeline's managed llama.cpp server." },
  { value: "pdftotext", label: "pdftotext (basic)", hint: "Fast, but equations are lost. Uses bundled poppler." },
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
          <h3 className="text-sm font-semibold text-gray-800 dark:text-gray-200 mb-1">Input & PDF Extraction</h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            Stage 0a. What the workflow takes as input, and how text is pulled from it before
            any LLM call.
          </p>
        </div>

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Input mode (per profile)
          </label>
          <select
            aria-label="Workflow input mode"
            value={inputMode}
            onChange={(e) => onChange({ input_mode: e.target.value })}
            className="w-full py-1.5 px-2 border border-gray-300 dark:border-gray-600 rounded-lg text-sm
                       text-gray-900 bg-white dark:bg-gray-800 dark:text-gray-200
                       focus:outline-none focus:ring-2 focus:ring-gray-400 focus:border-transparent transition-colors"
          >
            <option value="document">Document — a PDF, LaTeX, or Word file</option>
            <option value="folder">Folder — inventory a directory; steps Read files on demand</option>
            <option value="none">None — run from the step prompts alone</option>
          </select>
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-1.5 leading-relaxed">
            Selecting a folder in the main window uses folder mode automatically, whatever this
            is set to.
          </p>
        </div>

        {/* Extra named inputs */}
        <ExtraInputsEditor
          slots={extraction.extra_inputs ?? []}
          onChange={(extra_inputs) => onChange({ extra_inputs })}
        />

        {inputMode !== "document" ? (
          <div className="text-[11px] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
            Extraction settings below apply only to document inputs.
          </div>
        ) : null}

        <div>
          <label className="block text-xs font-medium text-gray-700 dark:text-gray-300 mb-1.5">
            Method (per profile)
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
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
          {hint && (
            <p
              className="text-[11px] mt-1.5 leading-relaxed text-gray-500 dark:text-gray-400"
            >
              {hint}
            </p>
          )}
        </div>

        <div className="text-[11px] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
          The chosen PDF method is authoritative: incomplete or failed extraction stops before
          orientation instead of silently switching engines. Parser-specific speed, memory, OCR,
          and image settings are configured once in Settings → Review & workflows → PDF Extraction. LaTeX inputs bypass
          PDF extraction.
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
  onPromptChange,
  onSchemaChange,
}: {
  prompt: string;
  schema?: Record<string, unknown> | null;
  inputMode: string;
  autoReview?: boolean;
  onPromptChange: (p: string) => void;
  onSchemaChange: (schema: Record<string, unknown> | null) => void;
}) {
  const resetRequest = useRef(0);
  const mounted = useRef(true);
  useEffect(() => () => {
    mounted.current = false;
    resetRequest.current += 1;
  }, []);

  const insertDefault = async (name: string) => {
    const request = ++resetRequest.current;
    try {
      const defaults = await invoke<{
        prompt: string;
        schema: Record<string, unknown>;
      }>("get_orientation_defaults", { name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange(defaults.prompt);
        onSchemaChange(defaults.schema);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        notify(
          `Failed to load the default orientation prompt: ${
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
      const defaults = await invoke<{
        schema: Record<string, unknown>;
      }>("get_orientation_defaults", { name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange("");
        onSchemaChange(defaults.schema);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        notify(
          `Failed to load the default orientation schema: ${
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
      // adaptive-agent count from the current schema.
      const defaults = await invoke<{
        prompt: string;
        schema: Record<string, unknown>;
      }>("get_auto_review_orientation_defaults");
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
            {autoReview ? "Orientation & Classification" : "Orientation Map"}
          </h3>
          <p className="text-xs text-gray-500 dark:text-gray-400 leading-relaxed">
            {autoReview ? (
              <>Stage 0b. One LLM call builds the paper orientation map and classifies the review it needs. Alongside sections, formal results, tables, and notation, the validated map includes a compact <span className="font-mono">review_plan</span> that selects the subject and method specialists assembled for this report and classifies the document genre shared with every reviewer.</>
            ) : (
              <>Stage 0b. One LLM call that builds a structured JSON survey of the input before any
              step runs — for a paper: sections, theorems, tables, notation. Steps that select the
              survey receive it via {"{orientation}"}, which keeps them grounded in what the input
              actually contains. The survey can use any JSON schema your prompt asks for.</>
            )}
          </p>
        </div>

        <div className="flex items-center gap-2">
          <span className="rounded bg-green-50 px-1.5 py-0.5 text-[10px] font-medium text-green-700 dark:bg-green-950/40 dark:text-green-300">
            Required
          </span>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            Runs before every workflow
          </span>
          <span className="ml-auto rounded-full bg-violet-50 px-2 py-0.5 text-[10px] font-medium text-violet-700 dark:bg-violet-950/40 dark:text-violet-300">
            {schema ? "Schema configured" : "Object-only schema"} · Schemas tab
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
              className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
              title="Restore the generated orientation & classification prompt and its matching output schema. A configured adaptive-agent count is kept."
            >
              Restore adaptive router prompt & schema
            </button>
          ) : (
            <>
              <button
                type="button"
                onClick={() => void insertDefault("orientation_generic")}
                className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the generic survey prompt (works for any input)."
              >
                Insert generic survey
              </button>
              <button
                type="button"
                onClick={() => void insertDefault("orientation_folder")}
                className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the folder survey prompt (explores the folder with the Read tool)."
              >
                Insert folder survey
              </button>
              <button
                type="button"
                onClick={() => void insertDefault("orientation")}
                className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100 transition-colors"
                title="Insert the paper-review survey prompt (sections, theorems, tables, notation)."
              >
                Insert paper survey
              </button>
              <button
                type="button"
                onClick={() => void useDefault()}
                disabled={prompt.trim() === ""}
                className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100
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
            ariaLabel="Orientation map prompt"
          />
        </div>
        {autoReview && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Catalog placeholders are populated from the live specialist manifests only when the
            workflow runs. The saved prompt stays compact; browse the Catalog tabs to inspect roles.
          </p>
        )}
        {prompt.trim() === "" && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Empty — using the bundled default: <span className="font-mono">prompts/orientation.md</span>{" "}
            for document inputs, <span className="font-mono">prompts/orientation_folder.md</span> for
            folder inputs (overridable at <span className="font-mono">~/.pipeline/prompts/</span>).
            Stock prompts adapt to the input mode; a customized prompt is used as-is.
          </p>
        )}
      </div>
    </div>
  );
}

// --- Step row ---

export const MemoizedExtractionEditor = memo(ExtractionEditor);
export const MemoizedOrientationEditor = memo(OrientationEditor);
