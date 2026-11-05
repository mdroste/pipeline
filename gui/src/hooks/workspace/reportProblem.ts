import { notify } from "../../components/DialogService";
import { notifyConversationProblem } from "../../lib/appNotifications";
import { conversationWarning } from "../../lib/workspaceTurnLabels";
import type { WorkspaceConversationRuntime } from "./useWorkspaceConversationRuntime";

type ProblemTarget = Pick<
  WorkspaceConversationRuntime,
  "missedProblemsRef" | "sessionRef" | "setError"
>;

const MAX_WARNINGS = 3;

/**
 * Show a problem on its conversation. A conversation the researcher has left
 * can't show it, so it raises a notice and waits for their return.
 */
export function reportProblem(
  { missedProblemsRef, sessionRef, setError }: ProblemTarget,
  sessionId: string,
  message: string,
) {
  if (sessionRef.current === sessionId) {
    setError(message);
    return;
  }
  missedProblemsRef.current.set(sessionId, message);
  notifyConversationProblem(sessionId, message);
}

/**
 * Show a warning from the assistant's connection. The event names only its
 * connection, so `source` is the conversation whose turn was running, if any.
 */
export function reportConnectionWarning(
  target: ProblemTarget & Pick<WorkspaceConversationRuntime, "setWarnings">,
  source: string | null,
  message: string,
) {
  const warning = conversationWarning(message);
  if (!warning) return;
  if (!source) notify(warning, "info");
  else if (target.missedProblemsRef.current.has(source))
    // A waiting problem is the more serious of the two, so it keeps its place.
    notifyConversationProblem(source, warning);
  else if (source !== target.sessionRef.current)
    reportProblem(target, source, warning);
  else
    target.setWarnings((old) =>
      [...old.filter((item) => item !== warning), warning].slice(-MAX_WARNINGS),
    );
}
