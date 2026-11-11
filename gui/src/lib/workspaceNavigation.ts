/** Presentation routes only. None of these preferences grants assistant access. */
export const workspaceDestinations = {
  overview: { label: "Project overview", section: "overview" },
  "project-settings": { label: "Project settings", section: "overview" },
  execution: { label: "Command settings", section: "overview" },
  exchange: { label: "Share project", section: "overview" },
  writing: { label: "Manuscript", section: "paper" },
  documents: { label: "Saved versions", section: "paper" },
  files: { label: "Files", section: "files" },
  reviews: { label: "Reviews & findings", section: "reviews" },
  responses: { label: "Responses", section: "reviews" },
  review: { label: "Send for review", section: "reviews" },
  edits: { label: "Review edits", section: "changes" },
  "version-control": { label: "Version control", section: "changes" },
  memory: { label: "Notes", section: "notes" },
  tasks: { label: "Action items", section: "notes" },
} as const;
export type WorkspaceDestination = keyof typeof workspaceDestinations;

/** Every view a researcher can open, in section order. */
export const openDestinations = Object.keys(
  workspaceDestinations,
) as WorkspaceDestination[];

export const workspaceSections = [
  {
    id: "overview",
    label: "Overview",
    description: "Where the paper stands and what to do next",
    destination: "overview",
    icon: "project",
  },
  {
    id: "paper",
    label: "Paper",
    description: "Write and build the manuscript, and keep its versions",
    destination: "writing",
    icon: "message",
  },
  {
    id: "files",
    label: "Files",
    description: "Everything in the project folder",
    destination: "files",
    icon: "attachment",
  },
  {
    id: "reviews",
    label: "Reviews",
    description: "Reviews, their findings, and replies to referees",
    destination: "reviews",
    icon: "research",
  },
  {
    id: "changes",
    label: "Changes",
    description: "Edits the assistant proposed, and version control",
    destination: "edits",
    icon: "check",
  },
  {
    id: "notes",
    label: "Notes",
    description: "Notes the assistant can use, and action items",
    destination: "memory",
    icon: "outline",
  },
] as const;
/**
 * A saved or linked value that names a view the project still offers. Views
 * retired in the projects overhaul (Theory, Experiments, Claims and the rest)
 * fail this check, so a layout, pin or link that names one opens the overview.
 */
export const isWorkspaceDestination = (
  value: unknown,
): value is WorkspaceDestination =>
  typeof value === "string" &&
  Object.prototype.hasOwnProperty.call(workspaceDestinations, value);
export const destinationsInSection = (destination: WorkspaceDestination) =>
  openDestinations.filter(
    (id) =>
      workspaceDestinations[id].section ===
      workspaceDestinations[destination].section,
  );

/** One line on what a view is for, shown at the top of its page. Views that
 * fill the pane with their own work, such as Files, have none. */
export const destinationHints: Partial<Record<WorkspaceDestination, string>> = {
  execution: "The local commands Pipeline may run for this project.",
  reviews: "Reviews of this project's documents and their findings.",
  responses: "Replies to referees and editors, comment by comment.",
  edits: "Proposed changes to accept or decline.",
  review: "Send a chosen version out for review.",
  exchange: "Package this project to share with someone else.",
  tasks: "What still needs doing in this project.",
  memory: "Notes the assistant can use in this project.",
};

// ---- Kind and stage -------------------------------------------------------

/** What sort of paper the project is. Sets defaults; never changes navigation. */
export const projectKinds = [
  { id: "empirical", label: "Empirical" },
  { id: "theory", label: "Theory" },
  { id: "survey", label: "Review or survey" },
  { id: "methods", label: "Methods" },
  { id: "essay", label: "Essay or humanities" },
  { id: "grant", label: "Grant proposal" },
  { id: "thesis", label: "Thesis chapter" },
  { id: "other", label: "Other" },
] as const;
export type ProjectKind = (typeof projectKinds)[number]["id"];
/** Where the paper is on its way to publication. Decides what the overview leads with. */
export const projectStages = [
  { id: "idea", label: "Idea" },
  { id: "drafting", label: "Drafting" },
  { id: "ready", label: "Ready to submit" },
  { id: "under_review", label: "Under review" },
  { id: "revising", label: "Revising" },
  { id: "accepted", label: "Accepted" },
] as const;
export type ProjectStage = (typeof projectStages)[number]["id"];
export const kindLabel = (id: string | null | undefined) =>
  projectKinds.find((kind) => kind.id === id)?.label ?? null;
export const stageLabel = (id: string | null | undefined) =>
  projectStages.find((stage) => stage.id === id)?.label ?? null;

/**
 * The tabs under a project section, always in the same order. Project setup
 * pages appear only once one of them is open, and Responses waits until the
 * paper is past drafting.
 */
export function sectionTabs(
  destination: WorkspaceDestination,
  stage?: string | null,
): WorkspaceDestination[] {
  if (destination === "overview") return ["overview"];
  return destinationsInSection(destination).filter(
    (id) =>
      id === destination ||
      (id !== "overview" &&
        !(id === "responses" && (stage === "idea" || stage === "drafting"))),
  );
}

export const defaultWorkspacePins: WorkspaceDestination[] = [
  "files",
  "writing",
  "reviews",
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
