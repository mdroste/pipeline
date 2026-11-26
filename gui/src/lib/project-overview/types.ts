import type {
  BuildReceipt,
  BuildRecord,
  ResponseRecord,
} from "../studioClient";
import type { ProjectIssueLedger } from "../reviewCollectionTypes";
import type { ProjectRecord } from "../projectClient";
import type { RepositoryStatus } from "../repositoryClient";
import type { RunSummary } from "../reviewTypes";
import type { TaskSummary } from "../taskTypes";
import type { WorkspaceDestination } from "../workspaceNavigation";

export type SignalSource =
  "builds" | "responses" | "automations" | "repository" | "reviews";
export interface SignalState {
  state: "loading" | "ready" | "error" | "not-applicable";
  fetchedAt: string | null;
  complete: boolean;
  scope: string;
  error?: string;
}
/** Values remain available after a failed refresh; sources describes their basis. */
export interface ProjectSignals {
  sources?: Partial<Record<SignalSource, SignalState>>;
  reviewLinks?: import("./projectPaths").ProjectReviewLink[];
  runs: RunSummary[] | null;
  ledgers: ProjectIssueLedger[] | null;
  builds: ProjectRecord<BuildRecord | BuildReceipt>[] | null;
  responses: ProjectRecord<ResponseRecord>[] | null;
  automations: TaskSummary[] | null;
  /** Null when the project folder is not a Git repository. */
  repository: RepositoryStatus | null;
}
export const NO_SIGNALS: ProjectSignals = {
  runs: null,
  ledgers: null,
  builds: null,
  responses: null,
  automations: null,
  repository: null,
};

export type OverviewAction =
  | { kind: "tab"; label: string; tab: WorkspaceDestination; targetId?: string }
  /** Opens a new conversation with this text as an unsent draft. */
  | {
      kind: "ask";
      label: string;
      prompt: string;
      context?: import("../deskClient").ContextItem[];
      target?: import("../deskClient").OpenResearchObject;
    }
  | {
      kind: "document";
      label: string;
      revisionId: string;
      mode?: "read" | "edit" | "compare";
    }
  | {
      kind: "file";
      label: string;
      location: import("../fileLinks").FileLocation;
    }
  | { kind: "run"; label: string; runId: string }
  | { kind: "review"; label: string }
  | { kind: "automation"; label: string; taskId: string }
  /** Opens a page in the researcher's browser. */
  | { kind: "link"; label: string; url: string }
  /** Asks the project's Git remote for new commits. Changes no files. */
  | { kind: "fetch"; label: string }
  /** Opens the project's notes on the overview. */
  | { kind: "notes"; label: string; noteId?: string };
export interface OverviewRow {
  id: string;
  text: string;
  detail?: string;
  attention?: boolean;
  category?: "blocker" | "decision" | "task" | "suggestion";
  /** Source revision, used to expire a deferred presentation preference. */
  version?: string;
  /**
   * A row that calls for something: "next" rows can lead the overview when
   * the paper's stage makes them the thing to do, and "needs" rows wait on a
   * decision from the researcher whatever the stage.
   */
  lane?: "next" | "needs";
  action: OverviewAction;
}
export interface OverviewBlock {
  id:
    | "next"
    | "needs"
    | "draft"
    | "documents"
    | "sync"
    | "round"
    | "repository"
    | "since";
  title: string;
  rows: OverviewRow[];
}
