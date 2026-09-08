/** Presentation routes only. None of these preferences grants assistant access. */
export const workspaceDestinations = {
  overview: { label: "Project brief", section: "overview" },
  decisions: { label: "Decisions & impact", section: "overview" },
  documents: { label: "Documents", section: "library" },
  files: { label: "Files", section: "library" },
  sources: { label: "Papers & sources", section: "library" },
  search: { label: "Search & collections", section: "library" },
  acquisition: { label: "Acquisition inbox", section: "library" },
  literature: { label: "Literature notes", section: "library" },
  results: { label: "Results", section: "analyses" },
  research: { label: "Experiments", section: "analyses" },
  bindings: { label: "Result links", section: "analyses" },
  data: { label: "Data & samples", section: "analyses" },
  plans: { label: "Captured execution", section: "analyses" },
  execution: { label: "Execution settings", section: "analyses" },
  grids: { label: "Specification grids", section: "analyses" },
  theory: { label: "Theory", section: "analyses" },
  symbols: { label: "Symbols & assumptions", section: "analyses" },
  writing: { label: "Manuscript", section: "writing" },
  responses: { label: "Responses", section: "writing" },
  edits: { label: "Edits & acceptance", section: "writing" },
  assets: { label: "Tables & figures", section: "writing" },
  campaigns: { label: "Revision campaigns", section: "writing" },
  delivery: { label: "Deliverables & kits", section: "writing" },
  sharing: { label: "Coauthors & replication", section: "writing" },
  review: { label: "Send for review", section: "writing" },
  exchange: { label: "Share project", section: "writing" },
  memory: { label: "Notes", section: "notes" },
  evidence: { label: "Claims & evidence", section: "notes" },
  tasks: { label: "Action items", section: "tasks" },
  checks: { label: "Scheduled checks", section: "tasks" },
} as const;
export type WorkspaceDestination = keyof typeof workspaceDestinations;
export const workspaceSections = [
  { id: "overview", label: "Overview", destination: "overview", icon: "project" },
  { id: "library", label: "Library", destination: "documents", icon: "attachment" },
  { id: "analyses", label: "Analyses", destination: "results", icon: "research" },
  { id: "writing", label: "Writing", destination: "writing", icon: "message" },
  { id: "notes", label: "Notes & evidence", destination: "memory", icon: "outline" },
  { id: "tasks", label: "Action items", destination: "tasks", icon: "check" },
] as const;
export const isWorkspaceDestination = (value: unknown): value is WorkspaceDestination =>
  typeof value === "string" && Object.prototype.hasOwnProperty.call(workspaceDestinations, value);
export const destinationsInSection = (destination: WorkspaceDestination) =>
  (Object.keys(workspaceDestinations) as WorkspaceDestination[]).filter(id =>
    workspaceDestinations[id].section === workspaceDestinations[destination].section);

export const defaultWorkspacePins: WorkspaceDestination[] = ["files", "writing", "results"];
export function loadWorkspacePins(workspaceId: string): WorkspaceDestination[] {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(`pipeline.workspace.pins.${workspaceId}`) ?? "null");
    if (raw && typeof raw === "object" && "version" in raw && raw.version === 1 && "pins" in raw && Array.isArray(raw.pins))
      return [...new Set(raw.pins.filter(isWorkspaceDestination))].slice(0, 4);
  } catch { /* Corrupt or inaccessible optional preferences use defaults. */ }
  return [...defaultWorkspacePins];
}
export function saveWorkspacePins(workspaceId: string, pins: WorkspaceDestination[]) {
  try { localStorage.setItem(`pipeline.workspace.pins.${workspaceId}`, JSON.stringify({ version: 1, pins })); }
  catch { /* Navigation preferences never block research work. */ }
}
