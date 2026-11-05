import useModalDialog from "../../hooks/useModalDialog";
import { Button } from "../../ui";
import ReportWorkspace from "../ReportWorkspace";
import type { WorkspaceTab } from "../report-workspace/types";
import {
  EXAMPLE_REPORT,
  EXAMPLE_REPORT_MARKDOWN,
  EXAMPLE_SUMMARY,
} from "./exampleReviewData";
import { EXAMPLE_EXTRACTED_TEXT } from "./exampleReviewPaper";

export const EXAMPLE_REVIEW_TITLE = "Example review";

const TAB_GUIDES: Record<WorkspaceTab, string> = {
  report:
    "This is what you get at the end of a review: numbered comments, grouped by type.",
  provenance:
    "This shows what ran, how long it took, and what the automatic checks found. The figures are made up.",
  issues:
    "Open a comment to check its evidence. The last comment misquotes the paper, and the quote check catches it.",
  sources:
    "This holds the paper as Pipeline read it, the plan that chose the reviewers, and each reviewer's own report.",
};

/** The real report screen, showing bundled sample data. */
export default function ExampleReview({
  onClose,
  backLabel,
}: {
  onClose: () => void;
  /** Shown when closing returns to the setup sheet. */
  backLabel?: string;
}) {
  const dialogRef = useModalDialog<HTMLDivElement>(onClose, true, true);

  return (
    <div
      data-testid="example-review"
      className="fixed inset-0 z-50 flex bg-scrim p-2 sm:p-6"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={EXAMPLE_REVIEW_TITLE}
        tabIndex={-1}
        className="flex w-full flex-col overflow-hidden rounded-ui-lg border border-line bg-raised text-ink shadow-ui-3 outline-none"
      >
        <ReportWorkspace
          report={EXAMPLE_REPORT}
          markdown={EXAMPLE_REPORT_MARKDOWN}
          extractedText={EXAMPLE_EXTRACTED_TEXT}
          summary={EXAMPLE_SUMMARY}
          notice={(tab) => (
            <div className="flex shrink-0 items-center gap-4 border-b border-line bg-sunken px-6 py-2.5">
              <div className="min-w-0 flex-1 text-ui-label">
                <p className="font-medium">
                  Example review of an invented paper. No model was called, and
                  nothing is saved.
                </p>
                <p className="text-ink-muted">{TAB_GUIDES[tab]}</p>
              </div>
              <Button variant="primary" onClick={onClose}>
                {backLabel ?? "Close example"}
              </Button>
            </div>
          )}
        />
      </div>
    </div>
  );
}
