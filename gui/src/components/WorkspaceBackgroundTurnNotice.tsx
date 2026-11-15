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
      className="ui-notice ui-notice--warning flex flex-wrap items-center gap-3"
    >
      <p className="min-w-0 flex-[1_1_12rem]">
        {waiting > 0
          ? `A response in ${conversation} is waiting for your approval.`
          : automation
            ? `An automation is running in ${conversation}. You can send a message here when it finishes.`
            : `A response is still running in ${conversation}. You can send a message here when it finishes.`}
      </p>
      <button
        type="button"
        onClick={automation ? onOpenAutomations : onOpen}
        className="ui-button ui-button--sm shrink-0"
      >
        {automation ? "Open Automations" : "Open conversation"}
      </button>
    </div>
  );
}
