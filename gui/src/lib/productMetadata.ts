/** Shared user-facing workspace metadata. Keep navigation and Help sourced
 * from this table so optional views cannot drift into contradictory copy. */
export const REPORT_WORKSPACE_TABS = [
  { id: "report", label: "Report", description: "the consolidated write-up", availability: "always" },
  { id: "provenance", label: "Provenance", description: "run, model, usage, and quality details", availability: "when report metadata is available" },
  { id: "issues", label: "Issues", description: "structured findings and human decisions", availability: "when the workflow publishes findings" },
  { id: "sources", label: "Sources", description: "saved source and run artifacts", availability: "always" },
] as const;
