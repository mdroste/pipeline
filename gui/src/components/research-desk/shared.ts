import type { OpenResearchObject } from "../../lib/deskClient";
export interface DeskProps {
  workspaceId: string;
  sessionId?: string | null;
  onOpen: (object: OpenResearchObject) => void;
  onError: (message: string) => void;
  onRefresh?: () => Promise<void>;
}
export { button, input, muted } from "../../ui/classes";
import { card as sharedCard } from "../../ui/classes";
export const card = `space-y-3 ${sharedCard}`;
