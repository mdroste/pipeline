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
  const manualMethods = [
    {
      value: "paddleocr-vl-full",
      label: "Local parser: PaddleOCR-VL 1.6 Full Parser",
      quality: "Best quality",
      qualityClass:
        "bg-emerald-100 text-emerald-700 dark:bg-emerald-950/70 dark:text-emerald-300",
      description:
        "Keeps reading order, headings, equations, and tables, including tables that span pages.",
    },
    {
      value: "llm",
      label: "Selected model",
      quality: "High quality",
      qualityClass:
        "bg-blue-100 text-blue-700 dark:bg-blue-950/70 dark:text-blue-300",
      description:
        "Reads the PDF a few pages at a time. It can lose more detail than PaddleOCR-VL, takes longer, and adds model costs.",
    },
    {
      value: "pdftotext",
      label: "pdftotext",
      quality: "Basic quality",
      qualityClass:
        "bg-gray-100 text-gray-600 dark:bg-neutral-800 dark:text-neutral-300",
      description:
        "The fastest method. It loses page layout and usually loses equations.",
    },
  ] as const;

  return (
    <>
      <SectionHeader
        title="PDF extraction"
        description="Choose how Pipeline reads PDFs. LaTeX files are read directly."
      />

      <div className="space-y-4">
        <Field label="PDF extraction method">
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
                  Uses PaddleOCR-VL Full Parser if it’s installed. Otherwise,
                  your selected model reads the PDF.
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

        <details className="settings-disclosure">
          <summary>
            Advanced parser settings{" "}
            <span>Performance & document structure</span>
          </summary>
          {!paddleInstalled && (
            <p className="settings-row-description mt-3">
              These settings apply only when Pipeline uses PaddleOCR-VL Full
              Parser.
            </p>
          )}
          <p className="settings-row-description mt-3">
            Changing any of these settings makes Pipeline read a PDF again
            instead of reusing its saved result.
          </p>
          <div className="space-y-6 pt-5">
            <div className="pl-1 border-l-2 border-gray-200 dark:border-neutral-700 ml-1">
              <SubsectionHeader label="Speed and memory" />
              <div className="space-y-3 pl-4">
                <Field
                  label="Concurrent pages"
                  help="How many pages to read at once. Automatic reads two on Apple Silicon and one on other computers. Each added page uses more memory."
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
                    <option value={1}>1 page (least memory)</option>
                    <option value={2}>2 pages (faster)</option>
                    <option value={3}>3 pages (needs more memory)</option>
                    <option value={4}>4 pages (fastest)</option>
                  </select>
                </Field>

                <Field
                  label="Image batch size"
                  help="Larger batches can process page images faster if you have enough GPU memory. OCR quality stays the same."
                >
                  <select
                    aria-label="PaddleOCR-VL image batch size"
                    value={settings.paddle_mtmd_batch_tokens}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        paddle_mtmd_batch_tokens: parseInt(e.target.value, 10),
                      })
                    }
                    className={selectClass}
                  >
                    <option value={0}>Automatic (recommended)</option>
                    <option value={512}>512 tokens (less memory)</option>
                    <option value={1024}>1,024 tokens</option>
                    <option value={2048}>2,048 tokens (faster)</option>
                    <option value={4096}>4,096 tokens (most memory)</option>
                  </select>
                </Field>

                <Field label="Maximum page output">
                  <select
                    aria-label="PaddleOCR-VL maximum page output"
                    value={settings.paddle_max_output_tokens}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        paddle_max_output_tokens: parseInt(e.target.value, 10),
                      })
                    }
                    className={selectClass}
                  >
                    <option value={2048}>2,048 tokens (shorter pages)</option>
                    <option value={4096}>4,096 tokens (recommended)</option>
                    <option value={8192}>
                      8,192 tokens (very dense pages)
                    </option>
                  </select>
                </Field>

                <Field
                  label="Page retries"
                  help="The parser retries pages with possible extraction errors before reporting a failure."
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
                    <option value={1}>1 retry (recommended)</option>
                    <option value={2}>2 retries</option>
                    <option value={3}>3 retries</option>
                  </select>
                </Field>

                <Field
                  label="Flash Attention"
                  help="Leave this on Automatic. Turn it off if recognition fails on your GPU."
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
              <SubsectionHeader label="Document structure" />
              <div className="space-y-3 pl-4">
                <Toggle
                  label="Layout detection and reading order"
                  description="Find the regions on each page and their reading order before reading the text."
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
                        paddle_full_layout_threshold_version: 1,
                      })
                    }
                    className={selectClass}
                  >
                    <option value={0.3}>0.30 (recommended)</option>
                    <option value={0.5}>0.50 (fewer regions)</option>
                    <option value={0.7}>0.70 (fewest regions)</option>
                  </select>
                </Field>
                <Toggle
                  label="Remove overlapping regions"
                  description="Drop duplicate regions before reading their text."
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
                      Keep outer region (recommended)
                    </option>
                    <option value="small">Keep inner region</option>
                    <option value="union">Keep both</option>
                  </select>
                </Field>
                <Toggle
                  label="Merge split regions"
                  description="Join regions that cross columns or are staggered down the page."
                  checked={settings.paddle_full_merge_layout_blocks}
                  onChange={(v) =>
                    setSettings({
                      ...settings,
                      paddle_full_merge_layout_blocks: v,
                    })
                  }
                />
                <Toggle
                  label="Read text inside images"
                  description="Read labels and other text in figures and images."
                  checked={settings.paddle_full_ocr_image_blocks}
                  onChange={(v) =>
                    setSettings({
                      ...settings,
                      paddle_full_ocr_image_blocks: v,
                    })
                  }
                />
                <Toggle
                  label="Save regions as Markdown"
                  description="Save tables, equations, lists, and other regions as separate Markdown blocks."
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
                  description="Join tables that continue onto the next page."
                  checked={settings.paddle_full_merge_tables}
                  onChange={(v) =>
                    setSettings({ ...settings, paddle_full_merge_tables: v })
                  }
                />
                <Toggle
                  label="Fix heading levels"
                  description="Use consistent heading levels throughout the document."
                  checked={settings.paddle_full_relevel_titles}
                  onChange={(v) =>
                    setSettings({
                      ...settings,
                      paddle_full_relevel_titles: v,
                    })
                  }
                />
                <Toggle
                  label="Keep equation numbers"
                  description="Keep equation numbers in the extracted text."
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

        <Field
          label="Extraction time limit"
          help="Time allowed to read a whole document, including retries. If time runs out, the review stops."
        >
          <select
            aria-label="PDF extraction time limit"
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
            <option value={1800}>30 minutes (recommended)</option>
            <option value={3600}>60 minutes</option>
          </select>
        </Field>
      </div>
    </>
  );
}
