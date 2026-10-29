import { useId, useMemo } from "react";
import useModalDialog from "../../hooks/useModalDialog";
import { Button, IconButton } from "../../ui";
import { Icon } from "../../ui/icons";
import IssuesTable from "../IssuesTable";
import {
  EXAMPLE_PAPER,
  EXAMPLE_SUMMARY,
  exampleIssues,
} from "./exampleReviewData";

export const EXAMPLE_REVIEW_TITLE =
  "Example Review — sample data, no model was called";

const LEGEND = [
  {
    term: "Severity",
    detail:
      "High, medium, or low: how much the finding could change the paper's conclusions.",
  },
  {
    term: "Category",
    detail: "The part of the paper a finding is about, such as Data.",
  },
  {
    term: "Evidence and quote checks",
    detail:
      "Open a finding to see the passages it cites and whether Pipeline found each quotation in the document. The check covers the quotation, not whether the comment is right.",
  },
  {
    term: "Decisions",
    detail:
      "Accept, reject, or mark findings done. In this example nothing is saved.",
  },
];

/** A read-only Review built from bundled sample data. */
export default function ExampleReview({
  onClose,
  backLabel,
}: {
  onClose: () => void;
  /** Shown when closing returns to the setup sheet. */
  backLabel?: string;
}) {
  const titleId = useId();
  const descriptionId = useId();
  const dialogRef = useModalDialog<HTMLDivElement>(onClose, true, true);
  const issues = useMemo(exampleIssues, []);

  return (
    <div
      data-testid="example-review"
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim p-2 sm:p-6"
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        className="flex max-h-[calc(100vh-1rem)] w-full max-w-5xl flex-col overflow-hidden rounded-ui-lg border border-line bg-raised text-ink shadow-ui-3 outline-none sm:max-h-[calc(100vh-3rem)]"
      >
        <header className="flex shrink-0 items-start gap-3 border-b border-line px-4 py-3 sm:px-6">
          <div className="min-w-0 flex-1">
            <span className="inline-flex rounded-full border border-line-strong px-2 py-0.5 text-ui-meta font-medium text-ink-muted">
              Sample data
            </span>
            <h2 id={titleId} className="mt-1.5 text-ui-title font-semibold">
              {EXAMPLE_REVIEW_TITLE}
            </h2>
            <p
              id={descriptionId}
              className="mt-0.5 text-ui-label text-ink-muted"
            >
              A fictional paper excerpt with sample findings, so you can see
              what a finished Review contains. Nothing here comes from a model,
              and nothing is added to your Review history.
            </p>
          </div>
          <IconButton label="Close example Review" onClick={onClose}>
            <Icon name="close" className="h-4 w-4" />
          </IconButton>
        </header>

        <div className="min-h-0 flex-1 space-y-6 overflow-y-auto px-4 py-5 sm:px-6">
          <section aria-labelledby={`${titleId}-summary`}>
            <h3
              id={`${titleId}-summary`}
              className="text-ui-body font-semibold"
            >
              Summary
            </h3>
            {EXAMPLE_SUMMARY.map((paragraph) => (
              <p key={paragraph} className="mt-1.5 max-w-3xl text-ui-label">
                {paragraph}
              </p>
            ))}
          </section>

          <section
            aria-labelledby={`${titleId}-legend`}
            className="rounded-ui-md bg-sunken p-4"
          >
            <h3
              id={`${titleId}-legend`}
              className="text-ui-label font-semibold"
            >
              How to read a Review
            </h3>
            <dl className="mt-2 grid gap-x-6 gap-y-2 sm:grid-cols-2">
              {LEGEND.map((item) => (
                <div key={item.term}>
                  <dt className="text-ui-meta font-medium">{item.term}</dt>
                  <dd className="text-ui-meta text-ink-muted">{item.detail}</dd>
                </div>
              ))}
            </dl>
          </section>

          <details className="rounded-ui-md border border-line" open>
            <summary className="cursor-pointer px-4 py-2.5 text-ui-label font-semibold">
              Paper excerpt (fictional)
            </summary>
            <article className="space-y-3 border-t border-line px-4 py-3">
              <header>
                <p className="text-ui-label font-semibold">
                  {EXAMPLE_PAPER.title}
                </p>
                <p className="text-ui-meta text-ink-muted">
                  {EXAMPLE_PAPER.byline}
                </p>
              </header>
              {EXAMPLE_PAPER.pages.map((page) => (
                <div key={page.page}>
                  <p className="text-ui-meta font-medium text-ink-muted">
                    Page {page.page} · {page.heading}
                  </p>
                  <p className="max-w-3xl text-ui-label">{page.text}</p>
                </div>
              ))}
            </article>
          </details>

          <section aria-label="Sample findings" className="-mx-2 sm:-mx-6">
            <IssuesTable issues={issues} runId={null} />
          </section>
        </div>

        <footer className="flex shrink-0 items-center justify-end gap-2 border-t border-line px-4 py-3 sm:px-6">
          <Button variant="primary" onClick={onClose}>
            {backLabel ?? "Close example"}
          </Button>
        </footer>
      </div>
    </div>
  );
}
