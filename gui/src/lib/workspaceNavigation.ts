/** Presentation routes only. None of these preferences grants assistant access. */
export const workspaceDestinations = {
  overview: { label: "Project overview", section: "overview" },
  "project-settings": { label: "Project settings", section: "overview" },
  decisions: { label: "Decisions & impact", section: "activity" },
  documents: { label: "Documents", section: "library" },
  files: { label: "Files", section: "library" },
  sources: { label: "Papers & sources", section: "library" },
  search: { label: "Search & collections", section: "library" },
  acquisition: { label: "Acquisition inbox", section: "library" },
  literature: { label: "Literature notes", section: "library" },
  results: { label: "Results", section: "analyze" },
  research: { label: "Experiments", section: "analyze" },
  bindings: { label: "Result links", section: "analyze" },
  data: { label: "Data & samples", section: "analyze" },
  plans: { label: "Captured execution", section: "analyze" },
  execution: { label: "Execution settings", section: "analyze" },
  grids: { label: "Specification grids", section: "analyze" },
  theory: { label: "Theory", section: "analyze" },
  symbols: { label: "Symbols & assumptions", section: "analyze" },
  writing: { label: "Manuscript", section: "write" },
  reviews: { label: "Reviews & findings", section: "write" },
  responses: { label: "Responses", section: "write" },
  edits: { label: "Edits & acceptance", section: "write" },
  assets: { label: "Tables & figures", section: "write" },
  delivery: { label: "Deliverables & kits", section: "write" },
  sharing: { label: "Coauthors & replication", section: "write" },
  review: { label: "Send for review", section: "write" },
  exchange: { label: "Share project", section: "write" },
  campaigns: { label: "Revision automations", section: "automate" },
  tasks: { label: "Action items", section: "automate" },
  checks: { label: "Scheduled checks", section: "automate" },
  memory: { label: "Research notes", section: "activity" },
  evidence: { label: "Claims & evidence", section: "activity" },
} as const;
export type WorkspaceDestination = keyof typeof workspaceDestinations;
export const workspaceSections = [
  {
    id: "overview",
    label: "Overview",
    description: "Resume work, research brief, and next steps",
    destination: "overview",
    icon: "project",
  },
  {
    id: "library",
    label: "Library",
    description: "Documents, files, papers, and search",
    destination: "documents",
    icon: "attachment",
  },
  {
    id: "analyze",
    label: "Analyze",
    description: "Data, experiments, theory, and results",
    destination: "results",
    icon: "research",
  },
  {
    id: "write",
    label: "Write",
    description: "Manuscript, revisions, review, and delivery",
    destination: "writing",
    icon: "message",
  },
  {
    id: "automate",
    label: "Automate",
    description: "Action items, recurring checks, and revision work",
    destination: "tasks",
    icon: "check",
  },
  {
    id: "activity",
    label: "Activity",
    description: "Notes, decisions, evidence, and change impact",
    destination: "memory",
    icon: "outline",
  },
] as const;
export const isWorkspaceDestination = (
  value: unknown,
): value is WorkspaceDestination =>
  typeof value === "string" &&
  Object.prototype.hasOwnProperty.call(workspaceDestinations, value);
export const destinationsInSection = (destination: WorkspaceDestination) =>
  (Object.keys(workspaceDestinations) as WorkspaceDestination[]).filter(
    (id) =>
      workspaceDestinations[id].section ===
      workspaceDestinations[destination].section,
  );

export const defaultWorkspacePins: WorkspaceDestination[] = [
  "files",
  "writing",
  "results",
];
export function loadWorkspacePins(workspaceId: string): WorkspaceDestination[] {
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem(`pipeline.workspace.pins.${workspaceId}`) ?? "null",
    );
    if (
      raw &&
      typeof raw === "object" &&
      "version" in raw &&
      raw.version === 1 &&
      "pins" in raw &&
      Array.isArray(raw.pins)
    )
      return [...new Set(raw.pins.filter(isWorkspaceDestination))].slice(0, 4);
  } catch {
    /* Corrupt or inaccessible optional preferences use defaults. */
  }
  return [...defaultWorkspacePins];
}
export function saveWorkspacePins(
  workspaceId: string,
  pins: WorkspaceDestination[],
) {
  try {
    localStorage.setItem(
      `pipeline.workspace.pins.${workspaceId}`,
      JSON.stringify({ version: 1, pins }),
    );
  } catch {
    /* Navigation preferences never block research work. */
  }
}
