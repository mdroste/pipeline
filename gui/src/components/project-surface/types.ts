import type { ContextItem } from "../../lib/deskClient";
import type {
  Workspace,
  ReviewHandoff,
  ConversationSnapshot,
  WorkbenchSession,
} from "../../lib/workbenchTypes";
import type { ProjectTab } from "./shared";

export type SelectionActionKind = "ask" | "note" | "task" | "revision";
export interface WorkspaceProjectSurfaceProps {
  /** Called once when a project with nothing but conversations is opened on its overview. */
  onConversationFirst?: () => void;
  /** The brief of a conversation-only project, for display beside its conversation. */
  onProjectBrief?: (brief: string | null) => void;
  onReviewHandoff?: (handoff: ReviewHandoff) => void;
  destination?: ProjectTab;
  onDestination?: (tab: ProjectTab) => void;
  navigationInSidebar?: boolean;
  snapshot?: ConversationSnapshot | null;
  onSnapshot?: (snapshot: ConversationSnapshot) => void;
  workspaceId: string;
  sessionId?: string | null;
  sessions?: WorkbenchSession[];
  onResumeSession?: (id: string) => Promise<void>;
  onStartConversation?: () => Promise<void>;
  onAllProjects?: () => void;
  onConversation: (
    text: string,
    checkpoint?: string,
    contextItems?: ContextItem[],
  ) => Promise<void>;
  onWorkspaceChanged: (workspace: Workspace) => void;
}
