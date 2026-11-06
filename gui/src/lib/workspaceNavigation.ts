/** Presentation routes only. None of these preferences grants assistant access. */
import type { IconName } from "../ui/icons";

export const workspaceDestinations = {
  chats: { label: "Chats", section: "chats" },
  "project-tools": { label: "Project tools", section: "tools" },
  overview: { label: "Project overview", section: "overview" },
  "project-settings": { label: "Project settings", section: "settings" },
  decisions: { label: "Decisions & impact", section: "notes" },
  documents: { label: "Documents", section: "files" },
  files: { label: "Files", section: "files" },
  "version-control": { label: "Version control", section: "files" },
  sources: { label: "Papers & sources", section: "sources" },
  search: { label: "Search & collections", section: "sources" },
  acquisition: { label: "Reading inbox", section: "sources" },
  literature: { label: "Literature notes", section: "sources" },
  results: { label: "Results", section: "analysis" },
  research: { label: "Experiments", section: "analysis" },
  bindings: { label: "Result links", section: "analysis" },
  data: { label: "Data & samples", section: "analysis" },
  plans: { label: "Saved analysis plans", section: "analysis" },
  execution: { label: "Command settings", section: "settings" },
  grids: { label: "Specification grids", section: "analysis" },
  theory: { label: "Theory", section: "analysis" },
  symbols: { label: "Symbols & assumptions", section: "analysis" },
  writing: { label: "Manuscript", section: "manuscript" },
  reviews: { label: "Reviews & findings", section: "feedback" },
  responses: { label: "Responses", section: "feedback" },
  edits: { label: "Review edits", section: "feedback" },
  assets: { label: "Tables & figures", section: "manuscript" },
  delivery: { label: "Drafts & templates", section: "manuscript" },
  sharing: { label: "Coauthors & replication", section: "tools" },
  review: { label: "Send for review", section: "feedback" },
  exchange: { label: "Share project", section: "tools" },
  campaigns: { label: "Revision automations", section: "tools" },
  tasks: { label: "Action items", section: "overview" },
  checks: { label: "Scheduled checks", section: "tools" },
  memory: { label: "Research notes", section: "notes" },
  evidence: { label: "Claims & evidence", section: "notes" },
} as const;
export type WorkspaceDestination = keyof typeof workspaceDestinations;
export const workspaceSections = [
  {
    id: "overview",
    label: "Overview",
    description: "Resume work and see what needs you",
    destination: "overview",
    icon: "home",
  },
  {
    id: "chats",
    label: "Chats",
    description: "Conversations in this project",
    destination: "chats",
    icon: "assistant",
  },
  {
    id: "files",
    label: "Files",
    description: "Your folder, imported documents, and version control",
    destination: "files",
    icon: "folder",
  },
  {
    id: "sources",
    label: "Sources",
    description: "Papers, collections, and reading notes",
    destination: "sources",
    icon: "book",
  },
  {
    id: "analysis",
    label: "Analysis",
    description: "Results, data, experiments, and theory",
    destination: "results",
    icon: "result",
  },
  {
    id: "manuscript",
    label: "Manuscript",
    description: "Your paper, tables, and figures",
    destination: "writing",
    icon: "manuscript",
  },
  {
    id: "feedback",
    label: "Reviews & edits",
    description: "Project findings, proposed edits, and responses",
    destination: "reviews",
    icon: "review",
  },
  {
    id: "notes",
    label: "Notes",
    description: "Research notes, decisions, and evidence",
    destination: "memory",
    icon: "notes",
  },
] as const satisfies readonly {
  id: string;
  label: string;
  description: string;
  destination: WorkspaceDestination;
  icon: IconName;
}[];

export function sectionLabel(view: WorkspaceDestination) {
  const id = workspaceDestinations[view].section;
  return (
    workspaceSections.find((section) => section.id === id)?.label ??
    (id === "settings" ? "Project settings" : "Project tools")
  );
}

export const isWorkspaceDestination = (
  value: unknown,
): value is WorkspaceDestination =>
  typeof value === "string" &&
  Object.prototype.hasOwnProperty.call(workspaceDestinations, value);
/** One line on what a view is for, shown at the top of its page. Views that
 * fill the pane with their own work, such as Files, have none. */
export const destinationHints: Partial<Record<WorkspaceDestination, string>> = {
  overview: "Your research brief, recent work, and next steps.",
  chats: "Conversations in this project.",
  files: "Read and edit the attached folder.",
  documents: "Read and compare imported document versions.",
  "version-control": "Inspect changes and history in the attached folder.",
  writing: "Read, edit, and build the current paper.",
  "project-settings":
    "Manage the project folder, paper, and reference results.",
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

/** These controls never reorder themselves when a specialist view is opened. */
const relatedViews: Record<string, readonly WorkspaceDestination[]> = {
  files: ["files", "documents", "version-control"],
  analysis: ["results", "data", "research"],
  feedback: ["reviews", "edits", "responses"],
  notes: ["memory", "decisions", "evidence"],
};
export const fixedViews = (id: WorkspaceDestination) =>
  relatedViews[workspaceDestinations[id].section] ?? [];

export const viewLinks: Partial<
  Record<WorkspaceDestination, readonly WorkspaceDestination[]>
> = {
  sources: ["acquisition", "literature", "search"],
  results: ["theory", "bindings"],
  writing: ["assets", "bindings", "delivery"],
  reviews: ["review"],
  tasks: ["checks", "campaigns"],
  "project-settings": ["execution"],
};
export const directoryGroup = (id: WorkspaceDestination) => {
  if (["sharing", "exchange", "execution", "project-settings"].includes(id))
    return "Sharing & settings";
  switch (workspaceDestinations[id].section) {
    case "files":
    case "sources":
      return "Files & reading";
    case "analysis":
      return "Analysis";
    case "manuscript":
    case "feedback":
      return "Writing & review";
    default:
      return "Project work";
  }
};
export const directoryGroups = [
  "Files & reading",
  "Analysis",
  "Writing & review",
  "Project work",
  "Sharing & settings",
] as const;
export const allProjectViews = Object.keys(
  workspaceDestinations,
) as WorkspaceDestination[];

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
