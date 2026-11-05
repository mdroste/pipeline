import { useRef, useState } from "react";
import type { QueuedRun } from "../../lib/types";
import Badge from "../../ui/Badge";
import { Icon } from "../../ui/icons";
import Popover from "../../ui/Popover";
import ReviewRunList from "../ReviewRunList";
import { NavIcon } from "./NavRows";

export interface ActivityTrayProps {
  /** The app's current page is the Activity page. */
  current: boolean;
  runs: QueuedRun[];
  runLimit: number;
  activeRunKey: string | null;
  unseenRuns: ReadonlySet<string>;
  workspaceActive: boolean;
  workspaceAttention: boolean;
  tasksAttention: boolean;
  onOpenRun?: (key: string) => void;
  onCancelRun?: (run: QueuedRun) => void;
  onDismissRuns?: (key?: string) => void;
  onConfigureRuns?: () => void;
  onOpenConversations?: () => void;
  onOpenAutomations?: () => void;
  onActivity?: () => void;
}

/**
 * Everything live in one place: running and queued reviews, finished reviews
 * not yet opened, conversations waiting for approval, and automation updates.
 */
export default function ActivityTray({
  compact = false,
  ...props
}: ActivityTrayProps & { compact?: boolean }) {
  const {
    current,
    runs,
    runLimit,
    activeRunKey,
    unseenRuns,
    workspaceActive,
    workspaceAttention,
    tasksAttention,
    onOpenRun,
    onCancelRun,
    onDismissRuns,
    onConfigureRuns,
    onOpenConversations,
    onOpenAutomations,
    onActivity,
  } = props;
  const [open, setOpen] = useState(false);
  const anchor = useRef<HTMLButtonElement>(null);
  const running =
    runs.filter((run) => run.status === "running").length +
    (workspaceActive ? 1 : 0);
  const queued = runs.filter((run) => run.status === "queued").length;
  const forYou =
    unseenRuns.size + (workspaceAttention ? 1 : 0) + (tasksAttention ? 1 : 0);
  const live = running + queued + forYou > 0;
  const summary = [
    running > 0 && `${running} running`,
    running === 0 && queued > 0 && `${queued} queued`,
    forYou > 0 && `${forYou} for you`,
  ]
    .filter(Boolean)
    .join(" · ");
  const close = () => setOpen(false);
  const choose = (action?: () => void) => () => {
    close();
    action?.();
  };
  return (
    <>
      {compact ? (
        <NavIcon
          ref={anchor}
          icon="activity"
          label={summary ? `Activity: ${summary}` : "Activity"}
          current={current}
          active={live}
          aria-haspopup="dialog"
          aria-expanded={open}
          onClick={() => setOpen((value) => !value)}
        >
          {live && (
            <span className="nav-icon-mark">
              <Badge
                kind={forYou > 0 ? "attention" : "running"}
                label={forYou > 0 ? "needs attention" : "running"}
              />
            </span>
          )}
        </NavIcon>
      ) : (
        <button
          ref={anchor}
          type="button"
          className="nav-row nav-activity"
          aria-current={current ? "page" : undefined}
          aria-haspopup="dialog"
          aria-expanded={open}
          data-live={live || undefined}
          title={summary ? `Activity: ${summary}` : "Activity"}
          onClick={() => setOpen((value) => !value)}
        >
          {running > 0 ? (
            <span className="flex h-[18px] w-[18px] shrink-0 items-center justify-center">
              <span aria-hidden="true" className="nav-spinner" />
            </span>
          ) : (
            <Icon name="activity" />
          )}
          <span className="nav-row-label">Activity</span>
          {summary && <span className="nav-activity-summary">{summary}</span>}
        </button>
      )}
      <Popover
        open={open}
        onClose={close}
        anchorRef={anchor}
        label="Activity"
        side={compact ? "right" : "top"}
        align={compact ? "end" : "start"}
        width={304}
        className="p-1.5"
      >
        <div className="nav-activity-panel">
          {(workspaceAttention || tasksAttention) && (
            <>
              <div className="nav-activity-label">For you</div>
              {workspaceAttention && (
                <button
                  type="button"
                  className="nav-activity-item"
                  onClick={choose(onOpenConversations)}
                >
                  <Badge kind="attention" label="needs attention" />A
                  conversation needs your approval
                </button>
              )}
              {tasksAttention && (
                <button
                  type="button"
                  className="nav-activity-item"
                  onClick={choose(onOpenAutomations)}
                >
                  <Badge kind="attention" label="needs attention" />
                  An automation has an update
                </button>
              )}
            </>
          )}
          {workspaceActive && (
            <>
              <div className="nav-activity-label">Conversations</div>
              <button
                type="button"
                className="nav-activity-item"
                onClick={choose(onOpenConversations)}
              >
                <span aria-hidden="true" className="nav-spinner" />
                The assistant is responding
              </button>
            </>
          )}
          {onOpenRun && (
            <ReviewRunList
              runs={runs}
              limit={runLimit}
              activeRunKey={activeRunKey}
              unseen={unseenRuns}
              onOpen={(key) => {
                close();
                onOpenRun(key);
              }}
              onCancel={(run) => onCancelRun?.(run)}
              onDismiss={(key) => onDismissRuns?.(key)}
              onConfigure={
                onConfigureRuns ? choose(onConfigureRuns) : undefined
              }
            />
          )}
          {!live && !runs.length && (
            <p className="nav-activity-empty">Nothing is running.</p>
          )}
          <button
            type="button"
            className="nav-activity-all"
            onClick={choose(onActivity)}
          >
            See all activity
            <Icon name="arrow" className="h-3.5 w-3.5" />
          </button>
        </div>
      </Popover>
    </>
  );
}
