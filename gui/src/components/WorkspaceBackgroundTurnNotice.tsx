/** A response still running, or waiting for approval, in a conversation the researcher has left. */
export default function WorkspaceBackgroundTurnNotice({
  title,
  waiting,
  onOpen,
  onOpenAutomations,
}: {
  title: string | null;
  /** Approvals that response is waiting on. */
  waiting: number;
  onOpen: () => void;
  /** Set for an Automation's turn, which the Automations page stops. */
  onOpenAutomations?: () => void;
}) {
  const conversation = title ? `“${title}”` : "another conversation";
  const automation = onOpenAutomations && waiting === 0;
  return (
    <div
      role="status"
      className="flex items-center gap-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-100"
    >
      <p className="min-w-0 flex-1">
        {waiting > 0
          ? `A response in ${conversation} is waiting for your approval.`
          : automation
            ? `An automation is running in ${conversation}. You can send a message here when it finishes.`
            : `A response is still running in ${conversation}. You can send a message here when it finishes.`}
      </p>
      <button
        type="button"
        onClick={automation ? onOpenAutomations : onOpen}
        className="shrink-0 rounded border border-amber-400 px-2 py-1 text-xs font-medium"
      >
        {automation ? "Open Automations" : "Open conversation"}
      </button>
    </div>
  );
}
