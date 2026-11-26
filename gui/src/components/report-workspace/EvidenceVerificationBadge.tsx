import type { EvidenceVerificationStatus } from "../../lib/issues";

const LABELS: Record<
  EvidenceVerificationStatus,
  { label: string; detail: string; tone: string }
> = {
  verified: {
    label: "Quote found",
    detail: "Pipeline found this text where it is cited.",
    tone: "border-success text-success",
  },
  verified_with_normalization: {
    label: "Quote found",
    detail:
      "Pipeline found this text where it is cited after ignoring punctuation, case, hyphenation, LaTeX markup, or an ellipsis.",
    tone: "border-success text-success",
  },
  partially_verified: {
    label: "Partial match",
    detail:
      "The text appears elsewhere in the document, its location does not exist, or only a close paraphrase was found.",
    tone: "border-warning text-warning",
  },
  not_found: {
    label: "Quote not found",
    detail:
      "Pipeline could not find this text in the extracted document. Check the source before relying on this comment.",
    tone: "border-danger text-danger",
  },
  not_checkable: {
    label: "Not checkable",
    detail:
      "There is no quote to check, it cites an outside source, or the cited file was not saved with the run.",
    tone: "border-gray-300 text-gray-600 dark:border-gray-700 dark:text-gray-400",
  },
};

/** States what Pipeline's own check of a quoted passage found. */
export function EvidenceVerificationBadge({
  status,
}: {
  status?: EvidenceVerificationStatus;
}) {
  if (!status) return null;
  const { label, detail, tone } = LABELS[status];
  const title = `${detail} This checks the quotation only, not whether the comment is correct.`;
  return (
    <span
      className={`inline-flex rounded-md border px-1.5 py-0.5 text-[0.625rem] font-medium ${tone}`}
      title={title}
      aria-label={`${label}. ${title}`}
    >
      {label}
    </span>
  );
}
