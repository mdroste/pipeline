import type { AppRoute } from "./router";
import type { ProjectHome } from "./projectTypes";
import type { Workspace } from "./workbench/sessionTypes";
import type { Issue } from "./issues";
import type { PaperRevision } from "./workbench/researchTypes";

export interface DocumentSearchSource {
  project: Workspace;
  title: string;
  revision: PaperRevision;
}

export interface ResearchSearchEntry {
  id: string;
  group: "Documents" | "Notes" | "Files" | "Findings";
  label: string;
  hint: string;
  text: string;
  route: AppRoute;
  /** Content search already matched this query; excerpts can be shortened. */
  matchedQuery?: string;
}

export function searchExcerpt(text: string, query = "", length = 140) {
  const compact = text.replace(/\s+/g, " ").trim();
  const term = query
    .trim()
    .split(/\s+/)
    .find((word) =>
      compact.toLocaleLowerCase().includes(word.toLocaleLowerCase()),
    );
  const match = term
    ? compact.toLocaleLowerCase().indexOf(term.toLocaleLowerCase())
    : 0;
  const start = Math.max(0, match - 35);
  return `${start ? "…" : ""}${compact.slice(start, start + length)}${compact.length > start + length ? "…" : ""}`;
}

export function projectSearchEntries(
  project: Workspace,
  home: ProjectHome,
): ResearchSearchEntry[] {
  const base = {
    page: "workspace" as const,
    projectId: project.id,
    surface: "project" as const,
  };
  return [
    ...home.papers
      .filter(({ paper, revision }) => !paper.archived && revision)
      .map(({ paper, revision }) => ({
        id: `document:${paper.id}`,
        group: "Documents" as const,
        label: paper.title,
        hint: `${project.name} · Version ${revision!.contentHash.slice(0, 8)}`,
        text: revision!.entrypoint,
        route: { ...base, destination: "documents", revisionId: revision!.id },
      })),
    ...home.notes
      .filter((note) => note.state !== "rejected" && note.state !== "retired")
      .map((note) => ({
        id: `note:${note.id}`,
        group: "Notes" as const,
        label: searchExcerpt(note.body, "", 80) || "Untitled note",
        hint: `${project.name} · ${note.kind.replaceAll("_", " ")} · Revision ${note.revision}`,
        text: note.body,
        route: { ...base, destination: "memory", noteId: note.id },
      })),
    ...(home.inventory?.body.files ?? [])
      .filter((file) => file.status !== "missing")
      .map((file) => ({
        id: `file:${project.id}:${file.path}`,
        group: "Files" as const,
        label: file.path.split(/[\\/]/).pop() || file.path,
        hint: `${project.name} · File list ${home.inventory!.body.capturedAt}`,
        text: file.path,
        route: { ...base, destination: "files", file: file.path },
      })),
  ];
}

export function findingSearchEntries(
  runId: string,
  title: string,
  issues: Issue[],
  project?: Workspace,
): ResearchSearchEntry[] {
  return issues.map((issue) => ({
    id: `finding:${runId}:${issue.id}`,
    group: "Findings",
    label: issue.title,
    hint: [project?.name, title, issue.severity, issue.section]
      .filter(Boolean)
      .join(" · "),
    text: issue.body,
    route: {
      page: "history",
      runId,
      findingId: issue.id,
      ...(project ? { projectId: project.id } : {}),
    },
  }));
}
