export interface PendingSubmission {
  sessionId: string;
  text: string;
  threadId?: string;
  turnId?: string;
  epoch?: number;
  /** Found running after its page closed, so no promise reports its outcome. */
  adopted?: boolean;
}
export interface SubmissionOwner {
  sessionId: string;
  threadId: string | null;
}
/** How a turn ended, kept until its submission can be told. */
export interface TurnEnd {
  status: string;
  reason: string | null;
}
export type FileChanges = ReadonlyMap<string, Record<string, unknown>>;
