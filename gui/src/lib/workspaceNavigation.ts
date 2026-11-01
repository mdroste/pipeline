/** Presentation routes only. None of these preferences grants assistant access. */
export const workspaceDestinations = {
  overview: { label: "Project overview", section: "overview" },
  "project-settings": { label: "Project settings", section: "overview" },
  decisions: { label: "Decisions & impact", section: "activity" },
  documents: { label: "Documents", section: "library" },
  files: { label: "Files", section: "library" },
  "version-control": { label: "Version control", section: "library" },
  sources: { label: "Papers & sources", section: "library" },
  search: { label: "Search & collections", section: "library" },
  acquisition: { label: "Reading inbox", section: "library" },
  literature: { label: "Literature notes", section: "library" },
  results: { label: "Results", section: "analyze" },
  research: { label: "Experiments", section: "analyze" },
  bindings: { label: "Result links", section: "analyze" },
  data: { label: "Data & samples", section: "analyze" },
  plans: { label: "Saved analysis plans", section: "analyze" },
  execution: { label: "Command settings", section: "analyze" },
  grids: { label: "Specification grids", section: "analyze" },
  theory: { label: "Theory", section: "analyze" },
  symbols: { label: "Symbols & assumptions", section: "analyze" },
  writing: { label: "Manuscript", section: "write" },
  reviews: { label: "Reviews & findings", section: "write" },
  responses: { label: "Responses", section: "write" },
  edits: { label: "Review edits", section: "write" },
  assets: { label: "Tables & figures", section: "write" },
  delivery: { label: "Drafts & templates", section: "write" },
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

/** One line on what a view is for, shown at the top of its page. Views that
 * fill the pane with their own work, such as Files, have none. */
export const destinationHints: Partial<Record<WorkspaceDestination, string>> = {
  sources: "Papers and sources saved in this project, with their versions.",
  search: "Search everything in this project and keep reading collections.",
  acquisition: "Papers waiting to be read, and ways to find more.",
  literature:
    "Your bibliography, citation notes, and comparisons across papers.",
  results: "Runs and the numbers they produced.",
  research: "Local commands, experiment notes, and saved results.",
  bindings: "Connect numbers in the manuscript to the results behind them.",
  data: "Datasets, data dictionaries, and analysis samples.",
  plans: "Analysis plans you have saved, ready to review and run.",
  execution: "The local commands Pipeline may run for this project.",
  grids: "Run one analysis across a set of specifications.",
  theory: "Theory notes, proofs, and the checks behind them.",
  symbols: "Notation and assumptions used across the project.",
  reviews: "Reviews of this project's documents and their findings.",
  responses: "Replies to referees and editors, comment by comment.",
  edits: "Proposed changes to accept or decline.",
  assets: "Tables and figures, and where each one comes from.",
  delivery: "Build drafts from templates and starter kits.",
  sharing: "Coauthor review and replication packages.",
  review: "Send a chosen version out for review.",
  exchange: "Package this project to share with someone else.",
  campaigns: "Revision rounds that run across several sessions.",
  tasks: "What still needs doing in this project.",
  checks: "Checks that run on a schedule and report back.",
  memory: "Notes the assistant can use in this project.",
  evidence: "Claims, and the evidence behind each one.",
  decisions: "Decisions, their reasons, and what each change affects.",
};

/** The views each section shows as tabs. The rest stay one click away in the
 * section's More menu and in project search. */
const sectionTabViews: ReadonlySet<WorkspaceDestination> = new Set([
  "documents",
  "files",
  "version-control",
  "sources",
  "results",
  "data",
  "research",
  "writing",
  "reviews",
  "assets",
  "tasks",
  "checks",
  "campaigns",
  "memory",
  "decisions",
  "evidence",
]);
export function sectionTabs(destination: WorkspaceDestination) {
  const views = destinationsInSection(destination);
  return {
    // The open view always shows, so the page never loses its own label.
    tabs: views.filter((id) => sectionTabViews.has(id) || id === destination),
    more: views.filter((id) => !sectionTabViews.has(id) && id !== destination),
  };
}

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
