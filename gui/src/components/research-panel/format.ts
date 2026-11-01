const SENDER_STATUS: Record<string, string> = {
  accepted: "accepted this",
  human_confirmed: "confirmed this",
  check_passed: "recorded a passed check",
};

/** A sender's status from a package, which is never the reader's own decision. */
export function importedLabel(status: string) {
  return `Imported: the sender ${SENDER_STATUS[status] ?? `marked this ${status}`}. That isn't your decision.`;
}

export function compactHash(value?: string | null) {
  return value ? `${value.slice(0, 10)}…${value.slice(-6)}` : "—";
}

const STORED_LABELS: Record<string, string> = {
  // Provenance of notes and claims.
  user: "You",
  // Evidence assessments.
  not_checked: "Not checked",
  model_assessed: "Assessed by the assistant",
  human_confirmed: "Confirmed by you",
  check_passed: "Check passed",
  check_failed: "Check failed",
  // How a source arrived and how much of it is held.
  local_pdf: "Local PDF",
  local_bibtex: "Local BibTeX file",
  local_file: "Local file",
  metadata: "Details only",
  abstract: "Abstract only",
  partial: "Part of the text",
  full: "Full text",
  // How a paper is stored.
  pdf: "PDF",
  docx: "Word document",
  latex_project: "LaTeX project",
  source_tree: "Folder of source files",
  // How a run ended.
  timed_out: "Timed out",
  outcome_unknown: "Outcome unknown",
};

/** A stored status or kind as a reader should see it. */
export function plainLabel(value: string) {
  const label = STORED_LABELS[value] ?? value.replaceAll("_", " ");
  return label.charAt(0).toUpperCase() + label.slice(1);
}
