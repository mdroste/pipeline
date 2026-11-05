import type { OpenResearchObject } from "../../lib/deskClient";
export interface DeskProps {
  workspaceId: string;
  sessionId?: string | null;
  onOpen: (object: OpenResearchObject) => void;
  onError: (message: string) => void;
  onRefresh?: () => Promise<void>;
  /** A write here succeeded, so other project views may need to reload. */
  onChanged?: () => void;
  /** The researcher started an action, so an earlier failure no longer applies. */
  onActionStart?: () => void;
  /** False while this retained view is hidden. */
  active?: boolean;
}
export { button, input, muted } from "../../ui/classes";
import "../project-surface/ProjectPage.css";
export const card = "project-section";
