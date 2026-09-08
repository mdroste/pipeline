import { useCallback, useState } from "react";

import type { EngineStatus, Settings } from "../../lib/types";

import EnginesPanel from "../EnginesPanel";

import {
  selectClass,
  SectionHeader,
  Field,
  SubsectionHeader,
  Toggle,
} from "./controls";
export function ExtractionSection({
  settings,
  setSettings,
  onSystemChange,
}: {
  settings: Settings;
  setSettings: (s: Settings) => void;
  onSystemChange?: () => void;
}) {
  const [paddleInstalled, setPaddleInstalled] = useState(false);
  const handleEngineStatusChange = useCallback((engines: EngineStatus[]) => {
    setPaddleInstalled(
      engines.some(
        (engine) => engine.id === "paddleocr-vl-parser" && engine.installed,
      ),
    );
  }, []);
  const automaticDescription = paddleInstalled
    ? "PaddleOCR-VL Full Parser is installed, so Pipeline will use it. If the parser is later removed, Pipeline will use LLM extraction."
    : "Uses PaddleOCR-VL Full Parser when it is installed; otherwise uses LLM extraction.";
  const manualMethods = [
    {
      value: "paddleocr-vl-full",
      label: "Local engine: PaddleOCR-VL 1.6 Full Parser",
      quality: "Best quality",
      qualityClass:
        "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/70 dark:text-emerald-300",
      description:
        "Highest expected fidelity. Layout-aware extraction preserves reading order, structured blocks, title hierarchy, formula metadata, and cross-page tables.",
    },
    {
      value: "llm",
      label: "LLM",
      quality: "High quality",
      qualityClass:
        "bg-blue-100 text-blue-700 dark:bg-blue-950/70 dark:text-blue-300",
      description:
        "Your selected model reads the PDF a few pages at a time. This may lose more detail than PaddleOCR-VL, take longer, and add model costs.",
    },
    {
      value: "pdftotext",
      label: "pdftotext",
      quality: "Basic quality",
      qualityClass:
        "bg-gray-100 text-gray-600 dark:bg-neutral-800 dark:text-neutral-300",
      description:
        "Fastest option, but layout is flattened and equations are typically lost.",
    },
  ] as const;

  return (
    <>
      <SectionHeader
        title="PDF Extraction"
        description="Choose how PDFs become readable input. LaTeX source is always extracted natively."
      />

      <div className="space-y-4">
        <Field label="PDF Extraction Method">
          <div>
            <label
              className={`flex cursor-pointer items-start gap-3 rounded-lg border px-3 py-2.5 transition-colors ${
                settings.pdf_extractor === "auto"
                  ? "border-gray-900 bg-gray-50 dark:border-neutral-300 dark:bg-neutral-800/50"
                  : "border-gray-200 hover:border-gray-300 dark:border-neutral-700 dark:hover:border-neutral-600"
              }`}
            >
              <input
                type="radio"
                name="pdf_extractor"
                value="auto"
                aria-label="Automatic — Recommended"
                checked={settings.pdf_extractor === "auto"}
                onChange={(e) =>
                  setSettings({ ...settings, pdf_extractor: e.target.value })
                }
                className="mt-0.5 accent-gray-900 dark:accent-gray-300"
              />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-sm font-medium text-gray-900 dark:text-neutral-100">
                    Automatic
                  </span>
                  <span className="rounded bg-gray-900 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-white dark:bg-neutral-200 dark:text-neutral-900">
                    Recommended
                  </span>
                </div>
                <div className="mt-0.5 text-xs text-gray-500 dark:text-neutral-400">
                  {automaticDescription}
                </div>
              </div>
            </label>

            <div className="mb-1.5 mt-3 flex items-center justify-between gap-3 px-1">
              <span className="text-[10px] font-semibold uppercase tracking-wider text-gray-500 dark:text-neutral-400">
                Manual methods
              </span>
              <span className="text-[10px] font-medium uppercase tracking-wider text-gray-500 dark:text-neutral-400">
                Best to basic ↓
              </span>
            </div>

            <div className="relative space-y-1.5 pl-4">
              <div
                aria-hidden="true"
                className="absolute bottom-3 left-[3px] top-3 w-0.5 rounded-full bg-gradient-to-b from-emerald-500 via-blue-400 to-gray-300 dark:from-emerald-400 dark:via-blue-500 dark:to-neutral-600"
              />
              {manualMethods.map(
                ({ value, label, quality, qualityClass, description }) => (
                  <label
                    key={value}
                    className={`flex cursor-pointer items-start gap-3 rounded-lg border px-3 py-2.5 transition-colors ${
                      settings.pdf_extractor === value
                        ? "border-gray-900 bg-gray-50 dark:border-neutral-300 dark:bg-neutral-800/50"
                        : "border-gray-200 dark:border-neutral-700 hover:border-gray-300 dark:hover:border-neutral-600"
                    }`}
                  >
                    <input
                      type="radio"
                      name="pdf_extractor"
                      value={value}
                      aria-label={`${label} — ${quality}`}
                      checked={settings.pdf_extractor === value}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          pdf_extractor: e.target.value,
                        })
                      }
                      className="mt-0.5 accent-gray-900 dark:accent-gray-300"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                        <span className="text-sm font-medium text-gray-900 dark:text-neutral-100">
                          {label}
                        </span>
                        <span
                          className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${qualityClass}`}
                        >
                          {quality}
                        </span>
                      </div>
                      <div className="text-xs text-gray-500 dark:text-neutral-400 mt-0.5">
                        {description}
                      </div>
                    </div>
                  </label>
                ),
              )}
            </div>
          </div>
        </Field>

        <div
          id="paddleocr-local-engine"
          tabIndex={-1}
          className="settings-anchor rounded-lg focus:outline-none focus-visible:ring-2 focus-visible:ring-gray-400"
        >
          <EnginesPanel
            onSystemChange={onSystemChange}
            onEngineStatusChange={handleEngineStatusChange}
          />
        </div>

        {paddleInstalled && (
          <details className="settings-disclosure">
            <summary>
              Advanced parser settings{" "}
              <span>Performance & document structure</span>
            </summary>
            <div className="space-y-6 pt-5">
              <div className="pl-1 border-l-2 border-gray-200 dark:border-neutral-700 ml-1">
                <SubsectionHeader
                  label="PaddleOCR-VL recognition server"
                  help="The Full Parser uses Pipeline's managed llama.cpp server for recognition; no separate llama.cpp installation is needed."
                />
                <div className="space-y-3 pl-4">
                  <Field
                    label="Concurrent pages"
                    help="Automatic uses two slots on Apple Silicon and one elsewhere. Each slot receives a full 16K context."
                  >
                    <select
                      aria-label="PaddleOCR-VL concurrent pages"
                      value={settings.paddle_page_concurrency}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_page_concurrency: parseInt(e.target.value, 10),
                        })
                      }
                      className={selectClass}
                    >
                      <option value={0}>Automatic (recommended)</option>
                      <option value={1}>1 page — lowest memory</option>
                      <option value={2}>2 pages — higher throughput</option>
                      <option value={3}>
                        3 pages — high-memory workstation
                      </option>
                      <option value={4}>4 pages — maximum throughput</option>
                    </select>
                  </Field>

                  <Field
                    label="Vision encoder batch"
                    help="Larger batches can speed image encoding when enough GPU memory is available. They do not reduce OCR resolution."
                  >
                    <select
                      aria-label="PaddleOCR-VL vision encoder batch"
                      value={settings.paddle_mtmd_batch_tokens}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_mtmd_batch_tokens: parseInt(
                            e.target.value,
                            10,
                          ),
                        })
                      }
                      className={selectClass}
                    >
                      <option value={0}>Automatic (recommended)</option>
                      <option value={512}>512 tokens — lower memory</option>
                      <option value={1024}>1,024 tokens — conservative</option>
                      <option value={2048}>
                        2,048 tokens — faster prefill
                      </option>
                      <option value={4096}>
                        4,096 tokens — highest peak memory
                      </option>
                    </select>
                  </Field>

                  <Field label="Maximum page output">
                    <select
                      aria-label="PaddleOCR-VL maximum page output"
                      value={settings.paddle_max_output_tokens}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_max_output_tokens: parseInt(
                            e.target.value,
                            10,
                          ),
                        })
                      }
                      className={selectClass}
                    >
                      <option value={2048}>2,048 tokens — shorter pages</option>
                      <option value={4096}>4,096 tokens — recommended</option>
                      <option value={8192}>
                        8,192 tokens — unusually dense pages
                      </option>
                    </select>
                  </Field>

                  <Field
                    label="Page retries"
                    help="Suspicious layout-aware pages are retried before the Full Parser records an extraction failure."
                  >
                    <select
                      aria-label="PaddleOCR-VL page retries"
                      value={settings.paddle_page_retries}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_page_retries: parseInt(e.target.value, 10),
                        })
                      }
                      className={selectClass}
                    >
                      <option value={0}>No retries</option>
                      <option value={1}>1 retry — recommended</option>
                      <option value={2}>2 retries</option>
                      <option value={3}>3 retries</option>
                    </select>
                  </Field>

                  <Field
                    label="Flash Attention"
                    help="Automatic is safest across platforms. Force it on when benchmarking a supported GPU; turn it off for compatibility troubleshooting."
                  >
                    <select
                      aria-label="PaddleOCR-VL Flash Attention"
                      value={settings.paddle_flash_attention}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_flash_attention: e.target.value,
                        })
                      }
                      className={selectClass}
                    >
                      <option value="auto">Automatic (default)</option>
                      <option value="on">On</option>
                      <option value="off">Off</option>
                    </select>
                  </Field>
                </div>
              </div>

              <div className="pl-1 border-l-2 border-gray-200 dark:border-neutral-700 ml-1">
                <SubsectionHeader
                  label="Full parser structure"
                  help="These options are included in the Full Parser cache fingerprint, so changing one creates a distinct cached result."
                />
                <div className="space-y-3 pl-4">
                  <Toggle
                    label="Layout detection and reading order"
                    description="Run PP-DocLayoutV3 before recognition and retain semantic regions, coordinates, and reading order."
                    checked={settings.paddle_full_layout_detection}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_layout_detection: v,
                      })
                    }
                  />
                  <Field label="Layout confidence threshold">
                    <select
                      aria-label="PaddleOCR-VL layout confidence threshold"
                      value={settings.paddle_full_layout_threshold}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_full_layout_threshold: parseFloat(
                            e.target.value,
                          ),
                        })
                      }
                      className={selectClass}
                    >
                      <option value={0.3}>0.30 — retain more regions</option>
                      <option value={0.5}>0.50 — recommended</option>
                      <option value={0.7}>0.70 — higher precision</option>
                    </select>
                  </Field>
                  <Toggle
                    label="Layout NMS"
                    description="Suppress overlapping layout detections before region recognition."
                    checked={settings.paddle_full_layout_nms}
                    onChange={(v) =>
                      setSettings({ ...settings, paddle_full_layout_nms: v })
                    }
                  />
                  <Field label="Overlapping layout boxes">
                    <select
                      aria-label="PaddleOCR-VL overlapping layout boxes"
                      value={settings.paddle_full_layout_merge_bboxes_mode}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          paddle_full_layout_merge_bboxes_mode: e.target.value,
                        })
                      }
                      className={selectClass}
                    >
                      <option value="large">
                        Keep outer region — recommended
                      </option>
                      <option value="small">Keep inner region</option>
                      <option value="union">Keep both</option>
                    </select>
                  </Field>
                  <Toggle
                    label="Merge layout blocks"
                    description="Join cross-column or vertically staggered regions before producing reading-order blocks."
                    checked={settings.paddle_full_merge_layout_blocks}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_merge_layout_blocks: v,
                      })
                    }
                  />
                  <Toggle
                    label="OCR text inside images"
                    description="Recognize labels and other text within image regions."
                    checked={settings.paddle_full_ocr_image_blocks}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_ocr_image_blocks: v,
                      })
                    }
                  />
                  <Toggle
                    label="Format block content"
                    description="Retain block-level Markdown for tables, formulas, lists, and other semantic regions."
                    checked={settings.paddle_full_format_block_content}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_format_block_content: v,
                      })
                    }
                  />
                  <Toggle
                    label="Merge tables across pages"
                    description="Reconstruct a continuing table as one logical table when page boundaries divide it."
                    checked={settings.paddle_full_merge_tables}
                    onChange={(v) =>
                      setSettings({ ...settings, paddle_full_merge_tables: v })
                    }
                  />
                  <Toggle
                    label="Relevel titles"
                    description="Reconstruct a consistent multi-level heading hierarchy across the document."
                    checked={settings.paddle_full_relevel_titles}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_relevel_titles: v,
                      })
                    }
                  />
                  <Toggle
                    label="Retain formula numbers"
                    description="Keep equation numbers in the Markdown and structured formula evidence."
                    checked={settings.paddle_full_show_formula_numbers}
                    onChange={(v) =>
                      setSettings({
                        ...settings,
                        paddle_full_show_formula_numbers: v,
                      })
                    }
                  />
                </div>
              </div>
            </div>
          </details>
        )}

        <Toggle
          label="Reuse verified extraction cache"
          description="Reuse exact source-and-settings matches. PaddleOCR-VL Full Parser reuses its validated structure and image cache; verified LLM transcriptions avoid another provider call."
          checked={settings.reuse_pdf_extraction_cache}
          onChange={(v) =>
            setSettings({ ...settings, reuse_pdf_extraction_cache: v })
          }
        />

        <Field
          label="Extraction time budget"
          help="Applies to the complete extraction stage, including retries. An incomplete document fails before orientation instead of silently continuing."
        >
          <select
            aria-label="PDF extraction time budget"
            value={settings.pdf_extraction_timeout_secs}
            onChange={(e) =>
              setSettings({
                ...settings,
                pdf_extraction_timeout_secs: parseInt(e.target.value, 10),
              })
            }
            className={selectClass}
          >
            <option value={300}>5 minutes</option>
            <option value={600}>10 minutes</option>
            <option value={900}>15 minutes</option>
            <option value={1800}>30 minutes — recommended</option>
            <option value={3600}>60 minutes</option>
          </select>
        </Field>
      </div>
    </>
  );
}
