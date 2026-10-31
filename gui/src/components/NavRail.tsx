import { useEffect, useRef, useState, type ReactNode, type Ref } from "react";
import useConversationIndex from "../hooks/useConversationIndex";
import type { AppPage } from "../lib/router";
import Badge from "../ui/Badge";
import IconButton from "../ui/IconButton";
import { Icon } from "../ui/icons";
import ActivityTray, { type ActivityTrayProps } from "./nav-rail/ActivityTray";
import NavFlyout from "./nav-rail/NavFlyout";
import { NavChild, NavGroup, NavIcon, NavRow } from "./nav-rail/NavRows";
import NewMenu from "./nav-rail/NewMenu";
import { useNavRailSlot } from "./nav-rail/navRailSlot";
import useNavSections, { type NavSection } from "./nav-rail/useNavSections";
import "./nav-rail/NavRail.css";
import ResizeHandle from "./ResizeHandle";

export const NAV_RAIL_WIDTH = { default: 240, min: 208, max: 320 };

export type { AppPage };

export interface RecentProject {
  id: string;
  name: string;
  root: string | null;
  missingRootAt: string | null;
  updatedAt: string;
}

interface Props extends Partial<
  Omit<ActivityTrayProps, "current" | "activeRunKey">
> {
  activePage: AppPage;
  hasCurrentRun: boolean;
  /** The review on screen. */
  activeRunKey?: string | null;
  isMac: boolean;
  isWindows?: boolean;
  dependenciesReady: boolean | null;
  dependenciesLoading: boolean;
  dependenciesError?: string | null;
  width: number;
  onResize: (width: number) => void;
  /** Folded to icons. */
  collapsed?: boolean;
  onToggleCollapsed?: () => void;
  toggleRef?: Ref<HTMLButtonElement>;
  onNewRun: () => void;
  onNewConversation?: () => void;
  onNewProject?: () => void;
  onSearch?: () => void;
  onNavigate: (page: AppPage) => void;
  recentProjects?: RecentProject[];
  recentProjectsLoading?: boolean;
  onOpenProject?: (projectId: string) => void;
  onOpenConversation?: (sessionId: string) => void;
  onDependencies: () => void;
}

// The button keeps one window position in the rail and in the bar above the
// icons, so a second click undoes the first. On macOS it sits in line with the
// traffic lights, above the window-wide drag strip. On Windows it sits in the
// row of the window controls.
const railToggleClass = (isMac: boolean, isWindows: boolean) =>
  `absolute ${
    isMac
      ? "left-[76px] top-0.5 z-40 !h-6 !w-6"
      : isWindows
        ? "left-3 top-0.5 z-10"
        : "left-3 top-2 z-10"
  }`;

/** Sits above the icon rail, so the window controls stay clear of the page. */
export function CollapsedNavBar({
  isMac,
  isWindows = false,
  onExpand,
  toggleRef,
}: {
  isMac: boolean;
  isWindows?: boolean;
  onExpand: () => void;
  toggleRef?: Ref<HTMLButtonElement>;
}) {
  return (
    <div
      data-testid="collapsed-nav-bar"
      data-tauri-drag-region={isMac || isWindows ? "deep" : undefined}
      className={`relative shrink-0 border-b border-gray-200/80 bg-gray-100/90 dark:border-neutral-800 dark:bg-[#101010] ${
        isMac ? "h-7" : isWindows ? "h-8" : "h-11"
      }`}
    >
      <IconButton
        ref={toggleRef}
        label="Expand sidebar"
        tooltipSide="bottom"
        size="sm"
        className={railToggleClass(isMac, isWindows)}
        onClick={onExpand}
      >
        <Icon name="sidebar" />
      </IconButton>
    </div>
  );
}

/** The newest conversations, read only while their section is open. */
function RecentConversations({
  onOpen,
}: {
  onOpen?: (sessionId: string) => void;
}) {
  const { conversations, loading } = useConversationIndex();
  if (!conversations.length)
    return (
      <p className="nav-empty">
        {loading ? "Loading conversations…" : "No conversations yet."}
      </p>
    );
  return (
    <>
      {conversations.slice(0, 3).map(({ session, projectName }) => (
        <NavChild
          key={session.id}
          label={session.title}
          title={
            projectName ? `${session.title} · ${projectName}` : session.title
          }
          onClick={() => onOpen?.(session.id)}
        />
      ))}
    </>
  );
}

const NO_RUNS: NonNullable<Props["runs"]> = [];
const NONE_UNSEEN: ReadonlySet<string> = new Set();

const pageSection = (page: AppPage, unfiled: boolean): NavSection | null => {
  if (page === "conversations") return "conversations";
  if (page === "workspace") return unfiled ? "conversations" : "projects";
  if (page === "project-index") return "projects";
  if (["main", "history", "batch", "pipeline", "gallery"].includes(page))
    return "reviews";
  return null;
};

export default function NavRail({
  activePage,
  hasCurrentRun,
  runs = NO_RUNS,
  runLimit = 1,
  activeRunKey = null,
  unseenRuns = NONE_UNSEEN,
  onOpenRun,
  onCancelRun,
  onDismissRuns,
  onConfigureRuns,
  workspaceActive = false,
  workspaceAttention = false,
  tasksAttention = false,
  isMac,
  isWindows = false,
  dependenciesReady,
  dependenciesLoading,
  dependenciesError,
  width,
  onResize,
  collapsed = false,
  onToggleCollapsed,
  toggleRef,
  onNewRun,
  onNewConversation,
  onNewProject,
  onSearch,
  onNavigate,
  recentProjects = [],
  recentProjectsLoading = false,
  onOpenProject,
  onOpenConversation,
  onDependencies,
  onOpenConversations,
  onOpenAutomations,
  onActivity,
}: Props) {
  const rail = useNavRailSlot();
  const workspace = activePage === "workspace" ? rail?.workspace : null;
  const unfiled = workspace?.projectId === null;
  const section = pageSection(activePage, unfiled);
  const { isOpen, toggle } = useNavSections(section);
  const [flyout, setFlyout] = useState<NavSection | null>(null);
  const flyoutAnchors = {
    conversations: useRef<HTMLButtonElement>(null),
    projects: useRef<HTMLButtonElement>(null),
    reviews: useRef<HTMLButtonElement>(null),
  };
  const dismissRef = rail?.dismissRef;
  useEffect(() => {
    if (!dismissRef) return;
    dismissRef.current = () => setFlyout(null);
    return () => {
      dismissRef.current = null;
    };
  }, [dismissRef]);
  useEffect(() => {
    if (!collapsed) setFlyout(null);
  }, [collapsed]);

  const reviewsRunning = runs.filter((run) => run.status === "running").length;
  const projectStatus =
    workspaceAttention || workspaceActive ? (
      <Badge
        kind={workspaceAttention ? "attention" : "running"}
        label={workspaceAttention ? "needs attention" : "running"}
      />
    ) : undefined;
  const reviewStatus = reviewsRunning > 0 && (
    <span className="nav-count" aria-label={`${reviewsRunning} running`}>
      <span aria-hidden="true" className="nav-spinner" />
      {reviewsRunning}
    </span>
  );
  const automationStatus = tasksAttention ? (
    <Badge kind="attention" label="Automation update" />
  ) : undefined;
  const slot = <div ref={rail?.setSlot} className="nav-slot" />;

  /** Every choice in a section leaves the icon rail's flyout. */
  const pick = (action?: () => void) => () => {
    setFlyout(null);
    action?.();
  };
  const conversationChildren =
    workspace && unfiled ? (
      slot
    ) : (
      <RecentConversations
        onOpen={(id) => {
          setFlyout(null);
          onOpenConversation?.(id);
        }}
      />
    );
  const projectChildren =
    workspace && !unfiled ? (
      slot
    ) : recentProjectsLoading && !recentProjects.length ? (
      <div
        role="status"
        aria-label="Loading recent projects"
        className="space-y-1 px-2 py-1"
      >
        {[0, 1].map((item) => (
          <span
            key={item}
            className="block h-6 animate-pulse rounded-md bg-gray-200/70 dark:bg-neutral-800"
          />
        ))}
      </div>
    ) : recentProjects.length ? (
      recentProjects
        .slice(0, 3)
        .map((project) => (
          <NavChild
            key={project.id}
            label={project.name}
            onClick={pick(() => onOpenProject?.(project.id))}
            title={
              project.missingRootAt
                ? `${project.name} · Folder unavailable`
                : project.name
            }
            suffix={
              project.missingRootAt ? (
                <Badge kind="attention" label="Folder unavailable" />
              ) : undefined
            }
          />
        ))
    ) : (
      <p className="nav-empty">Your projects will appear here.</p>
    );
  const reviewChildren = (
    <>
      <NavChild
        label="New review"
        current={activePage === "main" && !hasCurrentRun}
        onClick={pick(onNewRun)}
        title={
          runs.some((run) => run.status === "running")
            ? "Set up another review; it runs alongside or after the current ones"
            : "Start a new review"
        }
      />
      <NavChild
        label="History"
        current={activePage === "history" || activePage === "batch"}
        onClick={pick(() => onNavigate("history"))}
        title="Review history"
      />
      <NavChild
        label="Designer"
        current={activePage === "pipeline" || activePage === "gallery"}
        onClick={pick(() => onNavigate("pipeline"))}
        title="Design review steps, prompts, and templates"
      />
    </>
  );
  const sections: Record<
    NavSection,
    {
      label: string;
      icon: "assistant" | "project" | "review";
      page: AppPage;
      current: boolean;
      suffix?: ReactNode;
      children: ReactNode;
    }
  > = {
    conversations: {
      label: "Conversations",
      icon: "assistant",
      page: "conversations",
      current: activePage === "conversations",
      children: conversationChildren,
    },
    projects: {
      label: "Projects",
      icon: "project",
      page: "project-index",
      current: activePage === "project-index",
      suffix: projectStatus,
      children: projectChildren,
    },
    reviews: {
      label: "Reviews",
      icon: "review",
      page: "history",
      current: false,
      suffix: reviewStatus,
      children: reviewChildren,
    },
  };
  const group = (id: NavSection) => {
    const entry = sections[id];
    return (
      <NavGroup
        icon={entry.icon}
        label={entry.label}
        current={entry.current}
        active={section === id}
        open={isOpen(id)}
        onToggle={() => toggle(id)}
        onClick={() => onNavigate(entry.page)}
        suffix={entry.suffix}
      >
        {entry.children}
      </NavGroup>
    );
  };
  const groupIcon = (id: NavSection) => {
    const entry = sections[id];
    return (
      <NavIcon
        ref={flyoutAnchors[id]}
        icon={entry.icon}
        label={entry.label}
        active={section === id}
        aria-expanded={flyout === id}
        onClick={() => setFlyout((open) => (open === id ? null : id))}
      >
        {entry.suffix && (
          <span className="nav-icon-mark">
            {id === "reviews" ? (
              <Badge kind="running" label={`${reviewsRunning} running`} />
            ) : (
              entry.suffix
            )}
          </span>
        )}
      </NavIcon>
    );
  };
  const newMenu = (
    <NewMenu
      compact={collapsed}
      projectName={workspace?.projectName}
      onNewConversation={
        workspace?.newConversation ?? (() => onNewConversation?.())
      }
      onNewReview={onNewRun}
      onNewProject={() => onNewProject?.()}
      onNewAutomation={() => onNavigate("tasks")}
    />
  );
  const activity = (
    <ActivityTray
      compact={collapsed}
      current={activePage === "activity"}
      runs={runs}
      runLimit={runLimit}
      activeRunKey={activePage === "main" ? activeRunKey : null}
      unseenRuns={unseenRuns}
      workspaceActive={workspaceActive}
      workspaceAttention={workspaceAttention}
      tasksAttention={tasksAttention}
      onOpenRun={onOpenRun}
      onCancelRun={onCancelRun}
      onDismissRuns={onDismissRuns}
      onConfigureRuns={onConfigureRuns}
      onOpenConversations={onOpenConversations}
      onOpenAutomations={onOpenAutomations ?? (() => onNavigate("tasks"))}
      onActivity={onActivity}
    />
  );
  const needsSetup =
    !dependenciesLoading && (Boolean(dependenciesError) || !dependenciesReady);
  const systemLabel = dependenciesLoading
    ? "Checking system"
    : dependenciesError
      ? "Check failed"
      : dependenciesReady
        ? "System ready"
        : "Setup needed";
  const systemDot = (
    <button
      type="button"
      className="nav-status"
      aria-label={systemLabel}
      title={systemLabel}
      onClick={onDependencies}
    >
      <Badge
        kind={
          dependenciesLoading
            ? "idle"
            : dependenciesError
              ? "error"
              : dependenciesReady
                ? "ok"
                : "attention"
        }
        label=""
      />
    </button>
  );

  if (collapsed)
    return (
      <aside className="relative flex w-14 shrink-0 flex-col items-center border-r border-gray-200/80 bg-gray-100/90 px-2 pb-2.5 pt-2 [color-scheme:light] dark:border-neutral-800 dark:bg-[#101010] dark:[color-scheme:dark]">
        <nav
          aria-label="Primary"
          className="flex min-h-0 flex-1 flex-col items-center gap-0.5"
        >
          {newMenu}
          {onSearch && (
            <NavIcon
              icon="search"
              label="Search"
              tooltip="Search (⌘/Ctrl K)"
              onClick={onSearch}
            />
          )}
          <span className="h-1.5" />
          <NavIcon
            icon="home"
            label="Home"
            current={activePage === "home"}
            onClick={() => onNavigate("home")}
          />
          {groupIcon("conversations")}
          {groupIcon("projects")}
          {groupIcon("reviews")}
          <NavIcon
            icon="tasks"
            label="Automations"
            tooltip="Schedule and run research automations"
            current={activePage === "tasks"}
            onClick={() => onNavigate("tasks")}
          >
            {automationStatus && (
              <span className="nav-icon-mark">{automationStatus}</span>
            )}
          </NavIcon>
        </nav>
        <div className="flex shrink-0 flex-col items-center gap-0.5 border-t border-gray-200/80 pt-2 dark:border-neutral-800">
          {activity}
          <NavIcon
            icon="settings"
            label="Settings"
            current={activePage === "settings"}
            onClick={() => onNavigate("settings")}
          />
          <NavIcon
            icon="help"
            label="Help"
            current={activePage === "help"}
            onClick={() => onNavigate("help")}
          />
          {systemDot}
        </div>
        {flyout && (
          <NavFlyout
            label={sections[flyout].label}
            anchorRef={flyoutAnchors[flyout]}
            onClose={() => setFlyout(null)}
            onOpenPage={pick(() => onNavigate(sections[flyout].page))}
          >
            {sections[flyout].children}
          </NavFlyout>
        )}
      </aside>
    );

  return (
    <aside
      style={{ width }}
      className="relative flex shrink-0 flex-col border-r border-gray-200/80 bg-gray-100/90 px-2.5 pb-2.5 [color-scheme:light] dark:border-neutral-800 dark:bg-[#101010] dark:[color-scheme:dark]"
    >
      {(isMac || isWindows) && (
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-7" />
      )}

      {onToggleCollapsed && (
        <IconButton
          ref={toggleRef}
          label="Collapse sidebar"
          tooltipSide="bottom"
          size="sm"
          className={railToggleClass(isMac, isWindows)}
          onClick={onToggleCollapsed}
        >
          <Icon name="sidebar" />
        </IconButton>
      )}

      <div
        className={`flex shrink-0 items-center text-sm font-semibold tracking-[-0.01em] text-gray-900 dark:text-neutral-100 ${
          isMac ? "h-7 pl-[96px]" : isWindows ? "h-8 pl-8" : "h-11 pl-8"
        }`}
      >
        Pipeline
      </div>

      <div className="mb-3 mt-1.5 flex shrink-0 gap-1.5">
        {newMenu}
        {onSearch && (
          <IconButton
            label="Search"
            tooltip="Search (⌘/Ctrl K)"
            tooltipSide="bottom"
            className="!h-[34px] !w-[34px] border border-gray-200/80 dark:border-neutral-800"
            onClick={onSearch}
          >
            <Icon name="search" className="h-[17px] w-[17px]" />
          </IconButton>
        )}
      </div>

      <nav
        aria-label="Primary"
        className="-mx-1 min-h-0 flex-1 space-y-0.5 overflow-y-auto px-1 [scrollbar-width:thin]"
      >
        <NavRow
          icon="home"
          label="Home"
          current={activePage === "home"}
          onClick={() => onNavigate("home")}
        />
        {group("conversations")}
        {group("projects")}
        {group("reviews")}
        <NavRow
          icon="tasks"
          label="Automations"
          current={activePage === "tasks"}
          onClick={() => onNavigate("tasks")}
          suffix={automationStatus}
          title="Schedule and run research automations"
        />
      </nav>

      <div className="mt-2 shrink-0 space-y-1 border-t border-gray-200/80 pt-2 dark:border-neutral-800">
        {needsSetup && (
          <button
            type="button"
            className="nav-setup"
            aria-label={systemLabel}
            onClick={onDependencies}
          >
            <Badge kind={dependenciesError ? "error" : "attention"} label="" />
            <span className="min-w-0">
              {systemLabel}
              <small>See what is missing</small>
            </span>
          </button>
        )}
        {activity}
        <div className="flex items-center gap-0.5">
          <span className="min-w-0 flex-1">
            <NavRow
              icon="settings"
              label="Settings"
              current={activePage === "settings"}
              onClick={() => onNavigate("settings")}
            />
          </span>
          <IconButton
            label="Help"
            aria-current={activePage === "help" ? "page" : undefined}
            onClick={() => onNavigate("help")}
          >
            <Icon name="help" />
          </IconButton>
          {!needsSetup && systemDot}
        </div>
      </div>
      <ResizeHandle
        currentWidth={width}
        defaultWidth={NAV_RAIL_WIDTH.default}
        label="Resize primary navigation"
        min={NAV_RAIL_WIDTH.min}
        max={NAV_RAIL_WIDTH.max}
        onResize={onResize}
      />
    </aside>
  );
}
