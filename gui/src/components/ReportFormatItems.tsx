import {
  reportExportClient,
  type ReportFormat,
  type ReportFormatExport,
} from "../lib/reportExportClient";
import { notify } from "./DialogService";

type ReviewedFileKind = "docx" | "pdf";

/** The reviewed file's type, judged from the saved input path. */
export function reviewedFileKind(
  inputPath?: string | null,
): ReviewedFileKind | null {
  const lower = (inputPath ?? "").trim().toLowerCase();
  if (lower.endsWith(".docx")) return "docx";
  if (lower.endsWith(".pdf")) return "pdf";
  return null;
}

interface FormatItem {
  format: ReportFormat;
  label: string;
  description: string;
  requires?: ReviewedFileKind;
  unavailable?: string;
  progress: string;
}

const ITEMS: FormatItem[] = [
  {
    format: "html",
    label: "Export as HTML",
    description:
      "One file that opens in any browser, offline, with the findings, quotations, and filters.",
    progress: "Preparing HTML…",
  },
  {
    format: "docx",
    label: "Export as Word",
    description: "A new Word document with the report and findings.",
    progress: "Preparing Word document…",
  },
  {
    format: "docxComments",
    label: "Word with comments on the original",
    description:
      "A copy of the reviewed .docx with each finding as a comment on the quoted passage.",
    requires: "docx",
    unavailable: "Available when the reviewed file is a Word (.docx) file.",
    progress: "Adding comments to a copy…",
  },
  {
    format: "annotatedPdf",
    label: "PDF with highlights",
    description:
      "A copy of the reviewed PDF with each quoted passage highlighted and a note per finding.",
    requires: "pdf",
    unavailable: "Available when the reviewed file is a PDF.",
    progress: "Highlighting a copy of the PDF…",
  },
];

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function placementSummary(
  format: ReportFormat,
  result: ReportFormatExport,
): string {
  if (format !== "docxComments" && format !== "annotatedPdf") return "";
  const placed =
    format === "docxComments"
      ? "commented on the quoted passage"
      : "highlighted";
  const fallback =
    format === "docxComments"
      ? "listed in a summary comment at the start"
      : "added as page notes";
  const parts = [
    `${result.anchored} of ${result.findings} finding${result.findings === 1 ? "" : "s"} ${placed}`,
  ];
  if (result.unanchored > 0) {
    parts.push(`${result.unanchored} ${fallback}`);
  }
  return ` ${parts.join("; ")}.`;
}

interface Props {
  runId: string;
  inputPath?: string | null;
  busy: boolean;
  onStart: (progress: string) => void;
  onFinish: () => void;
}

/** Report-format export choices for a saved run. */
export default function ReportFormatItems({
  runId,
  inputPath,
  busy,
  onStart,
  onFinish,
}: Props) {
  const kind = reviewedFileKind(inputPath);

  const save = async (item: FormatItem) => {
    onStart(item.progress);
    try {
      const result = await reportExportClient.saveFormat(item.format, {
        runId,
      });
      if (result) {
        notify(
          `Saved ${fileName(result.path)}.${placementSummary(item.format, result)}`,
          "success",
        );
      }
    } catch (e: unknown) {
      notify(`Export failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      onFinish();
    }
  };

  return (
    <div role="group" aria-label="Report formats">
      {ITEMS.map((item) => {
        const available = !item.requires || item.requires === kind;
        return (
          <button
            key={item.format}
            role="menuitem"
            disabled={!available || busy}
            onClick={() => void save(item)}
            className="w-full rounded px-3 py-2 text-left hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent dark:hover:bg-gray-800"
          >
            <span className="block text-sm font-medium text-gray-900 dark:text-gray-100">
              {item.label}
            </span>
            <span className="block text-xs text-gray-500 dark:text-gray-400">
              {available ? item.description : item.unavailable}
            </span>
          </button>
        );
      })}
      <p className="px-3 pb-1 pt-0.5 text-[11px] text-gray-500 dark:text-gray-400">
        These files contain quotations from the manuscript and model-written
        text.
      </p>
    </div>
  );
}
