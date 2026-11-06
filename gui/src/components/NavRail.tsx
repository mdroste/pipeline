import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { loadProjectPins } from "../lib/projectIndex";
import { appEvents } from "../lib/appEvents";
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
import "./project-surface/FocusedProject.css";
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
  dependenciesReady: boolean | null;
  dependenciesLoading: boolean;
  dependenciesError?: string | null;
  width: number;
  onResize: (width: number) => void;
  /** Folded to icons. */
  collapsed?: boolean;
  onNewRun: () => void;
  onNewConversation?: () => void;
  onNewProject?: () => void;
  onSearch?: () => void;
  onNavigate: (page: AppPage) => void;
  recentProjects?: RecentProject[];
  recentProjectsLoading?: boolean;
  onOpenProject?: (projectId: string) => void;
  onDependencies: () => void;
}

const NO_RUNS: NonNullable<Props["runs"]> = [];
const NONE_UNSEEN: ReadonlySet<string> = new Set();

const pageSection = (page: AppPage): NavSection | null => {
  if (page === "workspace" || page === "project-index") return "projects";
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
  dependenciesReady,
  dependenciesLoading,
  dependenciesError,
  width,
  onResize,
  collapsed = false,
  onNewRun,
  onNewConversation,
  onNewProject,
  onSearch,
  onNavigate,
  recentProjects = [],
  recentProjectsLoading = false,
  onOpenProject,
  onDependencies,
  onOpenConversations,
  onOpenAutomations,
  onActivity,
}: Props) {
  const asideRef = useRef<HTMLElement>(null);
  const previousLayout = useRef({ collapsed, width });
  const motion = useRef<Animation | null>(null);
  useLayoutEffect(() => {
    const previous = previousLayout.current;
    previousLayout.current = { collapsed, width };
    const aside = asideRef.current;
    // Continue from the visible width when a second toggle interrupts the first.
    const from =
      motion.current?.playState === "running"
        ? aside?.getBoundingClientRect().width
        : previous.collapsed
          ? 56
          : previous.width;
    motion.current?.cancel();
    motion.current = null;
    if (
      !aside?.animate ||
      previous.collapsed === collapsed ||
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches
    )
      return;
    motion.current = aside.animate(
      [
        { width: `${from}px`, opacity: 0.7, clipPath: "inset(0 -4px 0 0)" },
        {
          width: `${collapsed ? 56 : width}px`,
          opacity: 1,
          clipPath: "inset(0 -4px 0 0)",
        },
      ],
      { duration: 180, easing: "cubic-bezier(0.2, 0, 0, 1)" },
    );
  }, [collapsed, width]);
  useEffect(() => () => motion.current?.cancel(), []);

  const rail = useNavRailSlot();
  const workspace = activePage === "workspace" ? rail?.workspace : null;
  const section = pageSection(activePage);
  const { isOpen, toggle } = useNavSections(section);
  const [flyout, setFlyout] = useState<NavSection | null>(null);
  const flyoutAnchors = {
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
  const [pins, setPins] = useState(loadProjectPins);
  useEffect(
    () =>
      appEvents.on("project-pins-changed", () => setPins(loadProjectPins())),
    [],
  );
  const projectChildren = workspace ? (
    slot
  ) : (
    <>
      {recentProjectsLoading && !recentProjects.length ? (
        <p className="nav-empty" role="status">
          Loading projects…
        </p>
      ) : (
        ["Pinned projects", "Recent projects"].map((label, index) => {
          const items = recentProjects
            .filter((p) => (index ? !pins.includes(p.id) : pins.includes(p.id)))
            .slice(0, index ? 3 : 6);
          return (
            items.length > 0 && (
              <section key={label}>
                <div className="focused-scope-label">{label}</div>
                {items.map((project) => (
                  <NavChild
                    key={project.id}
                    label={project.name}
                    onClick={pick(() => onOpenProject?.(project.id))}
                    title={
                      project.missingRootAt
                        ? project.name + " · Folder unavailable"
                        : project.name
                    }
                    suffix={
                      project.missingRootAt ? (
                        <Badge kind="attention" label="Folder unavailable" />
                      ) : undefined
                    }
                  />
                ))}
              </section>
            )
          );
        })
      )}
      {!recentProjectsLoading && !recentProjects.length && (
        <p className="nav-empty">Your projects will appear here.</p>
      )}
    </>
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
      suffix?: ReactNode;
      children: ReactNode;
    }
  > = {
    projects: {
      label: "Projects",
      icon: "project",
      page: "project-index",
      suffix: projectStatus,
      children: projectChildren,
    },
    reviews: {
      label: "Reviews",
      icon: "review",
      page: "history",
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
        active={section === id}
        open={isOpen(id)}
        onToggle={() => toggle(id)}
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
      <aside
        ref={asideRef}
        style={{ width: 56 }}
        className="relative flex shrink-0 flex-col items-center border-r border-gray-200/80 bg-gray-100/90 px-2 pb-2.5 pt-2 [color-scheme:light] dark:border-neutral-800 dark:bg-surface dark:[color-scheme:dark]"
      >
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
          <NavIcon
            icon="project"
            label="Projects"
            current={activePage === "project-index"}
            active={section === "projects"}
            onClick={() => onNavigate("project-index")}
          >
            {projectStatus && (
              <span className="nav-icon-mark">{projectStatus}</span>
            )}
          </NavIcon>
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
          {workspace && (
            <div className="mt-2 border-t border-line pt-2">
              <NavIcon
                ref={flyoutAnchors.projects}
                icon={workspace.projectId ? "folder-open" : "assistant"}
                label={workspace.projectName ?? "Unfiled chats"}
                tooltip={
                  workspace.projectName
                    ? "Project navigation: " + workspace.projectName
                    : "Browse unfiled chats"
                }
                aria-haspopup="dialog"
                aria-expanded={flyout === "projects"}
                onClick={() =>
                  setFlyout((value) =>
                    value === "projects" ? null : "projects",
                  )
                }
              />
            </div>
          )}
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
            label={
              flyout === "projects"
                ? (workspace?.projectName ?? "No project")
                : sections[flyout].label
            }
            heading={flyout === "projects" ? "Project navigation" : undefined}
            openLabel={flyout === "projects" ? "All projects" : undefined}
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
      ref={asideRef}
      style={{ width }}
      className="relative flex shrink-0 flex-col border-r border-gray-200/80 bg-gray-100/90 px-2.5 pb-2.5 [color-scheme:light] dark:border-neutral-800 dark:bg-surface dark:[color-scheme:dark]"
    >
      <div className="flex h-9 shrink-0 items-center pl-2 text-sm font-semibold tracking-[-0.01em] text-gray-900 dark:text-neutral-100">
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

      <nav aria-label="Primary" className="-mx-1 shrink-0 space-y-0.5 px-1">
        <NavRow
          icon="home"
          label="Home"
          current={activePage === "home"}
          onClick={() => onNavigate("home")}
        />
        <NavRow
          icon="project"
          label="Projects"
          current={activePage === "project-index"}
          active={section === "projects"}
          onClick={() => onNavigate("project-index")}
          suffix={projectStatus}
        />
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

      <div
        className="focused-project-scope"
        role="group"
        aria-label={
          workspace
            ? workspace.projectId
              ? "Current project"
              : "No project"
            : "Project shortcuts"
        }
      >
        {projectChildren}
      </div>
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
