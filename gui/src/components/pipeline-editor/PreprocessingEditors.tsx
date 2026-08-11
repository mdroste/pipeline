import { memo, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ExtractionConfig } from "../../lib/types";
import PromptEditor from "../PromptEditor";
import { MemoizedExtraInputsEditor as ExtraInputsEditor } from "./ProfileSettingsEditors";
import { outputSchemaError } from "./utils";

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
  const markerRetired = method === "marker";
  const hint = markerRetired
    ? "Marker is unavailable in Pipeline 0.9.0 because its compatible Python dependencies contain known security vulnerabilities. Choose a supported method before running this workflow."
    : EXTRACTION_METHODS.find((m) => m.value === method)?.hint;
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
            {markerRetired && (
              <option value="marker" disabled>
                Marker (unavailable — choose a replacement)
              </option>
            )}
            {EXTRACTION_METHODS.map((m) => (
              <option key={m.value} value={m.value}>{m.label}</option>
            ))}
          </select>
          {hint && (
            <p
              role={markerRetired ? "alert" : undefined}
              className={`text-[11px] mt-1.5 leading-relaxed ${
                markerRetired
                  ? "text-amber-700 dark:text-amber-300"
                  : "text-gray-500 dark:text-gray-400"
              }`}
            >
              {hint}
            </p>
          )}
        </div>

        <div className="text-[11px] text-gray-600 dark:text-gray-400 leading-relaxed border-t border-gray-100 dark:border-gray-800 pt-3">
          The chosen PDF method is authoritative: incomplete or failed extraction stops before
          orientation instead of silently switching engines. Parser-specific speed, memory, OCR,
          and image settings are configured once in Settings → PDF Extraction. LaTeX inputs bypass
          PDF extraction.
        </div>
      </div>
    </div>
  );
}

function OrientationEditor({
  useOrientation,
  prompt,
  schema,
  autoReview = false,
  onToggleUse,
  onPromptChange,
  onSchemaChange,
}: {
  useOrientation: boolean;
  prompt: string;
  schema?: Record<string, unknown> | null;
  autoReview?: boolean;
  onToggleUse: (v: boolean) => void;
  onPromptChange: (p: string) => void;
  onSchemaChange: (schema: Record<string, unknown> | null) => void;
}) {
  const resetRequest = useRef(0);
  const mounted = useRef(true);
  const [schemaText, setSchemaText] = useState(
    schema ? JSON.stringify(schema, null, 2) : "",
  );
  const [schemaError, setSchemaError] = useState<string | null>(null);
  useEffect(() => {
    setSchemaText(schema ? JSON.stringify(schema, null, 2) : "");
    setSchemaError(null);
  }, [schema]);
  useEffect(() => () => {
    mounted.current = false;
    resetRequest.current += 1;
  }, []);

  const insertDefault = async (name: string) => {
    const request = ++resetRequest.current;
    try {
      const template = await invoke<string>("get_default_prompt", { name });
      if (mounted.current && request === resetRequest.current) {
        onPromptChange(template);
      }
    } catch (error) {
      if (mounted.current && request === resetRequest.current) {
        alert(
          `Failed to load the default orientation prompt: ${
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

  const changeSchema = (next: string) => {
    setSchemaText(next);
    if (!next.trim()) {
      setSchemaError(null);
      onSchemaChange(null);
      return;
    }
    try {
      const parsed = JSON.parse(next) as Record<string, unknown>;
      const error = outputSchemaError(parsed);
      if (error) {
        setSchemaError(error);
        return;
      }
      setSchemaError(null);
      onSchemaChange(parsed);
    } catch (error) {
      setSchemaError(error instanceof Error ? error.message : "invalid JSON");
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
              <>Stage 0b. One LLM call builds the paper orientation map and classifies the review it needs. Alongside sections, formal results, tables, and notation, the validated map includes a compact <span className="font-mono">review_plan</span> that selects the subject and method specialists assembled for this run.</>
            ) : (
              <>Stage 0b. One LLM call that builds a structured JSON survey of the input before any
              step runs — for a paper: sections, theorems, tables, notation. Steps that select the
              survey receive it via {"{orientation}"}, which keeps them grounded in what the input
              actually contains. The survey can use any JSON schema your prompt asks for.</>
            )}
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            type="button"
            role="switch"
            aria-label="Build orientation map"
            aria-checked={useOrientation}
            onClick={() => onToggleUse(!useOrientation)}
            className={`w-8 h-5 rounded-full relative transition-colors shrink-0 ${
              useOrientation ? "bg-green-600" : "bg-gray-300 dark:bg-gray-600"
            }`}
          >
            <div className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform ${
              useOrientation ? "translate-x-3.5" : "translate-x-0.5"
            }`} />
          </button>
          <span className="text-sm font-medium text-gray-800 dark:text-gray-200">
            {autoReview ? "Orientation & classification" : "Orientation map"} {useOrientation ? "enabled" : "disabled"}
          </span>
        </div>
      </div>

      <div className="p-4 border-b border-gray-200 dark:border-gray-700 flex items-center justify-between">
        <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
          Prompt
        </label>
        <div className="flex items-center gap-3">
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
            onClick={() => changePrompt("")}
            disabled={prompt.trim() === ""}
            className="text-[10px] text-gray-600 hover:text-gray-900 dark:text-gray-400 dark:hover:text-gray-100
                       disabled:opacity-40 disabled:hover:text-gray-600 dark:disabled:hover:text-gray-400 transition-colors"
            title="Clear the override; the default template will be used."
          >
            Use default
          </button>
        </div>
      </div>
      <div className="flex-1 min-h-0 p-4 flex flex-col">
        <div className="flex-1 min-h-0">
          <PromptEditor
            value={prompt}
            onChange={changePrompt}
            context={{ kind: "orientation" }}
            ariaLabel="Orientation map prompt"
          />
        </div>
        {prompt.trim() === "" && (
          <p className="text-[11px] text-gray-500 dark:text-gray-400 mt-2 leading-relaxed shrink-0">
            Empty — using the bundled default: <span className="font-mono">prompts/orientation.md</span>{" "}
            for document inputs, <span className="font-mono">prompts/orientation_folder.md</span> for
            folder inputs (overridable at <span className="font-mono">~/.pipeline/prompts/</span>).
            Stock prompts adapt to the input mode; a customized prompt is used as-is.
          </p>
        )}
        <div className="mt-4 shrink-0 border-t border-gray-200 pt-4 dark:border-gray-700">
          <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
            Output JSON schema (optional)
          </label>
          <p className="mt-1 text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
            When set, Pipeline validates the orientation map and retries malformed or incomplete
            routing output before any review step runs.
          </p>
          <textarea
            aria-label="Orientation output JSON schema"
            value={schemaText}
            onChange={(event) => changeSchema(event.target.value)}
            rows={7}
            placeholder='{ "type": "object", "required": ["metadata"] }'
            className="mt-2 w-full resize-y rounded-lg border border-gray-300 bg-white px-2 py-1.5 font-mono text-xs text-gray-900 focus:outline-none focus:ring-1 focus:ring-gray-400 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
          />
          {schemaError && (
            <p role="alert" className="mt-1 text-[10px] text-red-600 dark:text-red-400">
              Invalid schema: {schemaError}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

// --- Step row ---

export const MemoizedExtractionEditor = memo(ExtractionEditor);
export const MemoizedOrientationEditor = memo(OrientationEditor);
