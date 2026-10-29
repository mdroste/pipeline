import type { EvidenceVerificationStatus } from "../../lib/issues";

const LABELS: Record<
  EvidenceVerificationStatus,
  { label: string; detail: string; tone: string }
> = {
  verified: {
    label: "Quote found",
    detail: "Pipeline found this text where it is cited.",
    tone: "border-emerald-300 text-emerald-800 dark:border-emerald-800 dark:text-emerald-300",
  },
  verified_with_normalization: {
    label: "Quote found",
    detail:
      "Pipeline found this text where it is cited after ignoring punctuation, case, hyphenation, LaTeX markup, or an ellipsis.",
    tone: "border-emerald-300 text-emerald-800 dark:border-emerald-800 dark:text-emerald-300",
  },
  partially_verified: {
    label: "Partial match",
    detail:
      "The text appears elsewhere in the document, its location does not exist, or only a close paraphrase was found.",
    tone: "border-amber-300 text-amber-800 dark:border-amber-800 dark:text-amber-300",
  },
  not_found: {
    label: "Quote not found",
    detail:
      "Pipeline could not find this text in the extracted document. Check the source before relying on this comment.",
    tone: "border-red-300 text-red-800 dark:border-red-800 dark:text-red-300",
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
      className={`inline-flex rounded-md border px-1.5 py-0.5 text-[10px] font-medium ${tone}`}
      title={title}
      aria-label={`${label}. ${title}`}
    >
      {label}
    </span>
  );
}
