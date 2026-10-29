/** A response still running, or waiting for approval, in a conversation the researcher has left. */
export default function WorkspaceBackgroundTurnNotice({
  title,
  waiting,
  onOpen,
}: {
  title: string | null;
  /** Approvals that response is waiting on. */
  waiting: number;
  onOpen: () => void;
}) {
  const conversation = title ? `“${title}”` : "another conversation";
  return (
    <div
      role="status"
      className="flex items-center gap-3 rounded-lg border border-amber-300 bg-amber-50 p-3 text-sm text-amber-950 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-100"
    >
      <p className="min-w-0 flex-1">
        {waiting > 0
          ? `A response in ${conversation} is waiting for your approval.`
          : `A response is still running in ${conversation}. You can send a message here when it finishes.`}
      </p>
      <button
        type="button"
        onClick={onOpen}
        className="shrink-0 rounded border border-amber-400 px-2 py-1 text-xs font-medium"
      >
        Open conversation
      </button>
    </div>
  );
}
