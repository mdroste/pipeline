import { type ReactNode } from "react";
import ResizeHandle from "./ResizeHandle";
import { Icon, type IconName } from "../ui/icons";
import Badge from "../ui/Badge";
import type { AppPage } from "../lib/router";

export const NAV_RAIL_WIDTH = { default: 216, min: 208, max: 320 };

export type { AppPage };

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
  current?: boolean;
}) {
  return (
    <button
      type="button"
      aria-current={current ? "page" : undefined}
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

function SectionHeading({ id, children }: { id: string; children: ReactNode }) {
  return (
    <h2
      id={id}
      className="px-3 pb-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-gray-400 dark:text-neutral-500"
    >
      {children}
    </h2>
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
          icon="project"
          label="Projects"
          onClick={() => onNavigate("project-index")}
          suffix={
            workspaceActive || workspaceAttention ? (
              <Badge
                kind={workspaceAttention ? "attention" : "running"}
                label={workspaceAttention ? "needs attention" : "running"}
              />
            ) : undefined
          }
        />
        <RailButton
          active={activePage === "tasks"}
          icon="tasks"
          label="Automations"
          onClick={() => onNavigate("tasks")}
          suffix={
            tasksAttention ? (
              <Badge kind="attention" label="Automation update" />
            ) : undefined
          }
          title="Schedule and run research automations"
        />

        <section aria-labelledby="recent-projects-heading" className="!mt-5">
          <SectionHeading id="recent-projects-heading">
            Recent projects
          </SectionHeading>
          <div className="space-y-1">
            {recentProjects.slice(0, 4).map((project) => (
              <RailButton
                key={project.id}
                nested
                active={false}
                current={false}
                icon="project"
                label={project.name}
                onClick={() => onOpenProject?.(project.id)}
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

        <section
          aria-labelledby="reviews-heading"
          className="!mt-5 border-t border-gray-200/80 pt-4 dark:border-neutral-800"
        >
          <SectionHeading id="reviews-heading">Reviews</SectionHeading>
          <div className="space-y-1">
            <RailButton
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
              active={
                activePage === "history" ||
                activePage === "batch" ||
                (activePage === "main" && hasCurrentRun)
              }
              icon="history"
              label="Review history"
              onClick={() => onNavigate("history")}
            />
            <RailButton
              active={activePage === "pipeline" || activePage === "gallery"}
              icon="designer"
              label="Review designer"
              onClick={() => onNavigate("pipeline")}
              title="Design review steps, prompts, and templates"
            />
          </div>
        </section>
      </nav>

      <div className="mt-auto shrink-0 space-y-1 border-t border-gray-200/80 pt-3 dark:border-neutral-800">
        {onActivity && (
          <RailButton
            active={activePage === "activity"}
            icon="activity"
            label="Activity"
            onClick={onActivity}
            suffix={
              tasksAttention ? (
                <Badge kind="attention" label="Automation update" />
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
