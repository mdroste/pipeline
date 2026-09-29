import { useEffect, useId, useState, type ReactNode } from "react";
import ResizeHandle from "./ResizeHandle";

export const NAV_RAIL_WIDTH = { default: 216, min: 208, max: 320 };

export type AppPage =
  | "home"
  | "main"
  | "workspace"
  | "project-index"
  | "tasks"
  | "pipeline"
  | "settings"
  | "help"
  | "history"
  | "batch"
  | "projects"
  | "gallery";

type IconName =
  | "home"
  | "new"
  | "workspace"
  | "reviews"
  | "tasks"
  | "activity"
  | "runs"
  | "projects"
  | "designer"
  | "help"
  | "settings";

export interface RecentProject {
  id: string;
  name: string;
  root: string | null;
  missingRootAt: string | null;
  updatedAt: string;
}

interface Props {
  activePage: AppPage;
  hasCurrentRun: boolean;
  runInProgress: boolean;
  workspaceActive?: boolean;
  workspaceAttention?: boolean;
  tasksAttention?: boolean;
  isMac: boolean;
  dependenciesReady: boolean | null;
  dependenciesLoading: boolean;
  dependenciesError?: string | null;
  width: number;
  onResize: (width: number) => void;
  onNewRun: () => void;
  onNavigate: (page: AppPage) => void;
  recentProjects?: RecentProject[];
  recentProjectsLoading?: boolean;
  onOpenProject?: (projectId: string) => void;
  onDependencies: () => void;
  onActivity?: () => void;
}

function Icon({ name }: { name: IconName }) {
  const common = {
    className: "h-[18px] w-[18px]",
    fill: "none",
    viewBox: "0 0 24 24",
    stroke: "currentColor",
    strokeWidth: 1.65,
    "aria-hidden": true as const,
  };

  switch (name) {
    case "home":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m4 10 8-6.5 8 6.5v8.25A1.75 1.75 0 0 1 18.25 20H5.75A1.75 1.75 0 0 1 4 18.25V10Z"
          />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M9.25 20v-6.25h5.5V20"
          />
        </svg>
      );
    case "reviews":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M14 3.5H6A1.5 1.5 0 0 0 4.5 5v14A1.5 1.5 0 0 0 6 20.5h12a1.5 1.5 0 0 0 1.5-1.5V9L14 3.5ZM14 3.5V9h5.5M8 14l2.5 2.5L16 11"
          />
        </svg>
      );
    case "tasks":
      return (
        <svg {...common}>
          <rect x="4" y="4" width="16" height="16" rx="3" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="m8 12 2.5 2.5L16 9"
          />
        </svg>
      );
    case "activity":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3 12h4l3-7 4 14 3-7h4"
          />
        </svg>
      );
    case "workspace":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M5 18.5 3.5 21V5A2 2 0 0 1 5.5 3h13a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H7l-2 1.5Z"
          />
          <path strokeLinecap="round" d="M7.5 8h9M7.5 12h6" />
        </svg>
      );
    case "new":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M12 5v14M5 12h14"
          />
        </svg>
      );
    case "runs":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3.5 4.5v5h5M3.8 9a8.25 8.25 0 1 1 .3 6M12 7.5V12l3 2"
          />
        </svg>
      );
    case "projects":
      return (
        <svg {...common}>
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M3.75 7.25h6l1.5 2h9v8.5a2 2 0 0 1-2 2H5.75a2 2 0 0 1-2-2V7.25Z"
          />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M5.75 7.25V5.5a1.5 1.5 0 0 1 1.5-1.5h4.25l1.5 2h4.75a1.5 1.5 0 0 1 1.5 1.5v1.75"
          />
        </svg>
      );
    case "designer":
      return (
        <svg {...common}>
          <circle cx="6" cy="6" r="2.25" />
          <circle cx="18" cy="12" r="2.25" />
          <circle cx="6" cy="18" r="2.25" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M8.25 6h2.25A3.5 3.5 0 0 1 14 9.5v0A2.5 2.5 0 0 0 16.5 12M8.25 18h2.25A3.5 3.5 0 0 0 14 14.5v0A2.5 2.5 0 0 1 16.5 12"
          />
        </svg>
      );
    case "help":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.25" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M9.85 9.35a2.3 2.3 0 1 1 3.32 2.07c-.72.4-1.17.83-1.17 1.58v.25M12 16.75h.01"
          />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="2.75" />
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19 13.25v-2.5l-2.05-.52a5.7 5.7 0 0 0-.55-1.32l1.08-1.82-1.77-1.77-1.82 1.08a5.7 5.7 0 0 0-1.32-.55L12.05 3h-2.5l-.52 2.05a5.7 5.7 0 0 0-1.32.55L5.9 4.52 4.12 6.29 5.2 8.11a5.7 5.7 0 0 0-.55 1.32L2.6 9.95v2.5l2.05.52c.13.46.31.9.55 1.32l-1.08 1.82 1.77 1.77 1.82-1.08c.42.24.86.42 1.32.55l.52 2.05h2.5l.52-2.05c.46-.13.9-.31 1.32-.55l1.82 1.08 1.77-1.77-1.08-1.82c.24-.42.42-.86.55-1.32L19 13.25Z"
          />
        </svg>
      );
  }
}

function RailButton({
  active,
  disabled,
  icon,
  label,
  onClick,
  suffix,
  title,
  nested = false,
  action = false,
  expanded,
  controls,
  current = active,
}: {
  active: boolean;
  disabled?: boolean;
  icon: IconName;
  label: string;
  onClick: () => void;
  suffix?: ReactNode;
  title?: string;
  nested?: boolean;
  action?: boolean;
  expanded?: boolean;
  controls?: string;
  current?: boolean;
}) {
  return (
    <button
      type="button"
      aria-current={current ? "page" : undefined}
      aria-expanded={expanded}
      aria-controls={controls}
      disabled={disabled}
      onClick={onClick}
      title={title ?? label}
      className={`group flex w-full items-center gap-2 rounded-lg py-2 text-left text-[13px] font-medium
                  transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                  disabled:cursor-not-allowed disabled:opacity-40 ${nested ? "px-2" : "px-3"} ${
                    action
                      ? "bg-gray-900 text-white shadow-sm hover:bg-gray-800 dark:bg-neutral-200 dark:text-neutral-950 dark:hover:bg-neutral-100"
                      : active
                        ? "bg-white text-gray-950 shadow-sm ring-1 ring-gray-200/80 dark:bg-neutral-800 dark:text-neutral-50 dark:ring-neutral-700"
                        : "text-gray-600 hover:bg-white/70 hover:text-gray-950 dark:text-neutral-400 dark:hover:bg-neutral-800/70 dark:hover:text-neutral-100"
                  }`}
    >
      <span
        className={`shrink-0 ${action ? "text-current" : active ? "text-gray-900 dark:text-neutral-100" : "text-gray-500 group-hover:text-gray-700 dark:text-neutral-400 dark:group-hover:text-neutral-200"}`}
      >
        <Icon name={icon} />
      </span>
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {suffix}
    </button>
  );
}

export default function NavRail({
  activePage,
  hasCurrentRun,
  runInProgress,
  workspaceActive = false,
  workspaceAttention = false,
  tasksAttention = false,
  isMac,
  dependenciesReady,
  dependenciesLoading,
  dependenciesError,
  width,
  onResize,
  onNewRun,
  onNavigate,
  recentProjects = [],
  recentProjectsLoading = false,
  onOpenProject,
  onDependencies,
  onActivity,
}: Props) {
  const toolsPage = [
    "main",
    "history",
    "batch",
    "projects",
    "pipeline",
    "gallery",
    "tasks",
  ].includes(activePage);
  const [toolsOpen, setToolsOpen] = useState(toolsPage);
  const toolsMenuId = useId();

  useEffect(() => {
    if (toolsPage) setToolsOpen(true);
  }, [toolsPage]);

  return (
    <aside
      style={{ width }}
      className={`relative flex shrink-0 flex-col border-r border-gray-200/80 bg-gray-100/90 px-3 pb-3 [color-scheme:light]
                  dark:border-neutral-800 dark:bg-[#101010] dark:[color-scheme:dark] ${isMac ? "pt-12" : "pt-4"}`}
    >
      {isMac && (
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-12" />
      )}

      <div className="mb-5 px-2">
        <span className="text-base font-semibold tracking-[-0.01em] text-gray-900 dark:text-neutral-100">
          Pipeline
        </span>
      </div>

      <nav
        aria-label="Primary"
        className="min-h-0 overflow-y-auto pb-4 space-y-1 [scrollbar-width:thin]"
      >
        <RailButton
          active={activePage === "home"}
          icon="home"
          label="Home"
          onClick={() => onNavigate("home")}
        />
        <RailButton
          active={activePage === "workspace" || activePage === "project-index"}
          icon="projects"
          label="Projects"
          onClick={() => onNavigate("project-index")}
          suffix={
            workspaceActive || workspaceAttention ? (
              <span
                aria-label={workspaceAttention ? "needs attention" : "running"}
                className={`h-1.5 w-1.5 rounded-full ${workspaceAttention ? "bg-amber-500" : "bg-blue-500"}`}
              />
            ) : undefined
          }
        />

        <section aria-labelledby="recent-projects-heading" className="!mt-5">
          <h2
            id="recent-projects-heading"
            className="px-3 pb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-gray-400 dark:text-neutral-500"
          >
            Recent projects
          </h2>
          <div className="space-y-1">
            {recentProjects.slice(0, 4).map((project) => (
              <RailButton
                key={project.id}
                nested
                active={false}
                current={false}
                icon="projects"
                label={project.name}
                onClick={() => onOpenProject?.(project.id)}
                title={
                  project.missingRootAt
                    ? `${project.name} · Folder unavailable`
                    : project.name
                }
                suffix={
                  project.missingRootAt ? (
                    <span
                      aria-label="Folder unavailable"
                      className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-500"
                    />
                  ) : undefined
                }
              />
            ))}
            {recentProjectsLoading && !recentProjects.length ? (
              <div
                role="status"
                aria-label="Loading recent projects"
                className="space-y-2 px-2 py-1"
              >
                {[0, 1, 2].map((item) => (
                  <span
                    key={item}
                    className="block h-8 animate-pulse rounded-lg bg-gray-200/70 dark:bg-neutral-800"
                  />
                ))}
              </div>
            ) : (
              !recentProjects.length && (
                <p className="px-3 py-1 text-xs leading-5 text-gray-400 dark:text-neutral-500">
                  Your recent work will appear here.
                </p>
              )
            )}
          </div>
        </section>

        <div
          role="group"
          aria-label="Tools"
          className="!mt-5 border-t border-gray-200/80 pt-4 dark:border-neutral-800"
        >
          <h2>
            <RailButton
              active={toolsPage}
              current={false}
              icon="designer"
              label="Tools"
              onClick={() => setToolsOpen((open) => !open)}
              title={`${toolsOpen ? "Hide" : "Show"} advanced tools`}
              expanded={toolsOpen}
              controls={toolsMenuId}
              suffix={
                <svg
                  aria-hidden="true"
                  viewBox="0 0 20 20"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  className={`h-4 w-4 shrink-0 transition-transform ${toolsOpen ? "rotate-90" : ""}`}
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="m7.5 5 5 5-5 5"
                  />
                </svg>
              }
            />
          </h2>
          <div
            id={toolsMenuId}
            hidden={!toolsOpen}
            className="ml-[21px] mt-1 space-y-1 border-l border-gray-300/70 pl-2 dark:border-neutral-700"
          >
            <RailButton
              nested
              active={activePage === "tasks"}
              icon="tasks"
              label="Automations"
              onClick={() => onNavigate("tasks")}
              suffix={
                tasksAttention ? (
                  <span
                    aria-label="Automation update"
                    className="h-1.5 w-1.5 rounded-full bg-amber-500"
                  />
                ) : undefined
              }
              title="Schedule and run research automations"
            />
            <RailButton
              nested
              action
              active={activePage === "main" && !hasCurrentRun}
              disabled={runInProgress}
              icon="new"
              label="New review"
              onClick={onNewRun}
              title={
                runInProgress
                  ? "A review is already running"
                  : "Start a new review"
              }
            />
            <RailButton
              nested
              active={activePage === "history"}
              icon="runs"
              label="Review history"
              onClick={() => onNavigate("history")}
            />
            <RailButton
              nested
              active={activePage === "projects"}
              icon="projects"
              label="Review collections"
              onClick={() => onNavigate("projects")}
              title="Organize related review reports"
            />
            <div className="!mt-2 border-t border-gray-200/80 pt-2 dark:border-neutral-800">
              <RailButton
                nested
                active={activePage === "pipeline" || activePage === "gallery"}
                icon="designer"
                label="Review designer"
                onClick={() => onNavigate("pipeline")}
                title="Design review steps, prompts, and templates"
              />
            </div>
          </div>
        </div>
      </nav>

      <div className="mt-auto shrink-0 space-y-1 border-t border-gray-200/80 pt-3 dark:border-neutral-800">
        {onActivity && (
          <RailButton
            active={false}
            current={false}
            icon="activity"
            label="Activity"
            onClick={onActivity}
            suffix={
              tasksAttention ? (
                <span
                  aria-label="Automation update"
                  className="h-1.5 w-1.5 rounded-full bg-amber-500"
                />
              ) : undefined
            }
          />
        )}
        <RailButton
          active={activePage === "help"}
          icon="help"
          label="Help"
          onClick={() => onNavigate("help")}
        />
        <RailButton
          active={activePage === "settings"}
          icon="settings"
          label="Settings"
          onClick={() => onNavigate("settings")}
        />
        <button
          type="button"
          onClick={onDependencies}
          className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-xs text-gray-500
                     transition-colors hover:bg-white/70 hover:text-gray-800 focus-visible:outline-none
                     focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-neutral-400 dark:hover:bg-neutral-800/70
                     dark:hover:text-neutral-200"
        >
          <span
            aria-hidden="true"
            className={`h-1.5 w-1.5 rounded-full ${
              dependenciesLoading
                ? "bg-gray-400"
                : dependenciesError
                  ? "bg-red-500"
                  : dependenciesReady
                    ? "bg-emerald-500"
                    : "bg-amber-500"
            }`}
          />
          <span className="truncate">
            {dependenciesLoading
              ? "Checking system"
              : dependenciesError
                ? "Check failed"
                : dependenciesReady
                  ? "System ready"
                  : "Setup needed"}
          </span>
        </button>
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
