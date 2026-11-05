/** Shared user-facing workspace metadata. Keep navigation and Help sourced
 * from this table so optional views cannot drift into contradictory copy. */
export const REPORT_WORKSPACE_TABS = [
  {
    id: "report",
    label: "Report",
    description: "The combined write-up from all the reviewers.",
  },
  {
    id: "provenance",
    label: "Report details",
    description: "The models, usage, and quality checks for this review.",
  },
  {
    id: "issues",
    label: "Issues",
    description:
      "Findings you can accept, reject, or annotate. Appears when the review produces findings.",
  },
  {
    id: "sources",
    label: "Sources",
    description:
      "The extracted text, page images, and each reviewer's full response.",
  },
] as const;
