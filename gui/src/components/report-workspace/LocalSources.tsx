import { useEffect, useState, type ComponentType } from "react";
import type { ArtifactSelectionRequest } from "../../lib/artifactTypes";
import {
  renderSpecialistMarkdown,
  renderStructuredMarkdown,
} from "../../lib/issues";
import { renderSurvey } from "../../lib/surveyMarkdown";
import type { PipelineReport } from "../../lib/types";

const pageHeading = (page: number) => `Page ${page}`;

/** Whether extracted text has the page headings an evidence link can jump to. */
export function hasPageHeadings(extractedText?: string): boolean {
  return /^#{1,6} Page \d+\s*$/m.test(extractedText ?? "");
}

export default function LocalSources({
  ReportViewer,
  extractedText,
  report,
  selectionRequest = null,
}: {
  ReportViewer: ComponentType<{
    markdown: string;
    scrollRequest?: { key: number; heading: string } | null;
  }>;
  extractedText: string;
  report: PipelineReport;
  selectionRequest?: ArtifactSelectionRequest | null;
}) {
  const options = [
    { id: "extracted", label: "Extracted text", markdown: extractedText },
    {
      id: "orientation",
      label: "Orientation map",
      markdown: renderSurvey(report.orientation),
    },
    ...report.step_outputs.map((step) => ({
      id: `step:${step.step_id}`,
      label: step.step_label,
      // Specialist findings artifacts render back to their readable referee
      // form; other structured artifacts get a generic schema-aware view.
      // The raw JSON stays inspectable in the artifact explorer.
      markdown: `# ${step.step_label}\n\n**Phase:** ${step.phase} · **Agent:** ${step.agent || "default"}\n\n---\n\n${
        renderSpecialistMarkdown(step.raw_text) ??
        (step.structured_json
          ? renderStructuredMarkdown(step.raw_text)
          : null) ??
        step.raw_text
      }`,
    })),
  ];
  const [selected, setSelected] = useState(options[0]?.id ?? "");
  const active = options.find((option) => option.id === selected) ?? options[0];
  const requestedPage = selectionRequest?.page;

  useEffect(() => {
    if (requestedPage) setSelected("extracted");
  }, [requestedPage, selectionRequest?.key]);

  return (
    <div className="flex h-full min-h-0">
      <nav className="w-56 shrink-0 border-r border-gray-200 p-3 dark:border-gray-800">
        <p className="mb-2 px-2 text-[11px] font-semibold uppercase tracking-[0.12em] text-gray-500 dark:text-gray-400">
          Sources
        </p>
        {options.map((option) => (
          <button
            key={option.id}
            type="button"
            onClick={() => setSelected(option.id)}
            className={`mb-0.5 w-full truncate rounded-lg px-2.5 py-1.5 text-left text-xs ${
              option.id === active?.id
                ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                : "text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-800"
            }`}
          >
            {option.label}
          </button>
        ))}
      </nav>
      <div className="min-w-0 flex-1">
        {active && (
          <ReportViewer
            markdown={active.markdown}
            scrollRequest={
              requestedPage && selectionRequest && active.id === "extracted"
                ? {
                    key: selectionRequest.key,
                    heading: pageHeading(requestedPage),
                  }
                : null
            }
          />
        )}
      </div>
    </div>
  );
}
