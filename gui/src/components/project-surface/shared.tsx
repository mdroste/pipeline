import type { ReactNode } from "react";
import type {
  Annotation,
  HomeSettings,
  ProjectAction,
  ProjectHome,
  ProjectRecord,
  ResearchTask,
} from "../../lib/projectClient";
import type {
  PaperWithRevision,
  ResearchExecution,
  ResearchNote,
  Workspace,
} from "../../lib/workbenchTypes";

export type ProjectTab =
  import("../../lib/workspaceNavigation").WorkspaceDestination;

export const button =
  "rounded-md border border-gray-300 px-3 py-1.5 text-xs hover:bg-gray-50 disabled:opacity-40 dark:border-neutral-700 dark:hover:bg-neutral-800";
export const primaryButton =
  "rounded-md bg-gray-900 px-3 py-1.5 text-xs font-medium text-white hover:bg-gray-700 disabled:opacity-40 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-neutral-300";
export const linkButton =
  "text-xs underline decoration-gray-400 underline-offset-2 hover:decoration-current disabled:opacity-40";
export const input =
  "w-full rounded-md border border-gray-300 bg-transparent px-3 py-2 text-sm dark:border-neutral-700";
export const card =
  "rounded-xl border border-gray-200 bg-white p-5 dark:border-neutral-800 dark:bg-neutral-950";
export const muted = "text-xs text-gray-500 dark:text-neutral-400";
export const notice =
  "rounded-md border border-amber-300 bg-amber-50 p-3 text-xs text-amber-900 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-200";

export const op = () => `project-${crypto.randomUUID()}`;

export const NOTE_KINDS: Array<[ResearchNote["kind"], string]> = [
  ["question", "Question"],
  ["assumption", "Assumption"],
  ["decision", "Decision"],
  ["next_step", "Next step"],
  ["notation", "Notation"],
  ["handoff", "Handoff"],
];
export const noteKindLabel = (kind: string) =>
  NOTE_KINDS.find(([value]) => value === kind)?.[1] ??
  kind.replaceAll("_", " ");

export const TASK_STATUSES: Array<[string, string]> = [
  ["open", "Open"],
  ["investigating", "In progress"],
  ["deferred", "Later"],
  ["completed", "Done"],
  ["rejected", "Dropped"],
];
export const isOpenTask = (status: string) =>
  !["completed", "rejected"].includes(status);

const dateFormat = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});
/** Renders an ISO timestamp for reading; unparseable values are shown as stored. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? iso : dateFormat.format(parsed);
}
export const folderName = (path: string) =>
  path
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() || path;

export const versionLabel = (paper: PaperWithRevision) =>
  paper.revision
    ? `${paper.paper.title} · ${formatDate(paper.revision.capturedAt)}`
    : paper.paper.title;
export const earlierVersionLabel = (revisionId: string) =>
  `Earlier version · ${revisionId.slice(-8)}`;
export const runLabel = (run: ResearchExecution) =>
  `${run.adapter} · ${formatDate(run.endedAt ?? run.startedAt ?? run.createdAt)}`;

/** Ordered list of versions referenced by the project that no longer have a paper row. */
export function earlierVersions(data: ProjectHome): string[] {
  const referenced = [
    data.settings.body.manuscriptRevisionId,
    ...data.anchors.map((a) => a.body.selection.revisionId),
  ].filter((v): v is string => Boolean(v));
  return [...new Set(referenced)].filter(
    (id) => !data.papers.some((p) => p.revision?.id === id),
  );
}

/** Shared state and actions the tab components receive from the surface. */
export interface SurfaceApi {
  workspaceId: string;
  data: ProjectHome;
  workspace: Workspace | null;
  act: (action: ProjectAction) => Promise<void>;
  run: (operation: () => Promise<unknown>) => Promise<void>;
  saveSettings: (patch: Partial<HomeSettings>) => Promise<void>;
  reportError: (message: string) => void;
  openAnnotation: (annotation: ProjectRecord<Annotation>) => void;
  setTab: (tab: ProjectTab) => void;
  taskId: string;
  setTaskId: (id: string, selected?: ProjectRecord<ResearchTask>) => void;
}

export function Card({
  title,
  action,
  children,
  className = "",
}: {
  title: string;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`${card} space-y-3 ${className}`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="font-semibold">{title}</h2>
        {action}
      </div>
      {children}
    </section>
  );
}
