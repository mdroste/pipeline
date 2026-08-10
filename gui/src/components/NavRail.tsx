import type { ReactNode } from "react";
import ResizeHandle from "./ResizeHandle";

export type AppPage =
  | "main"
  | "pipeline"
  | "settings"
  | "help"
  | "history"
  | "batch";

type IconName =
  | "new"
  | "current"
  | "runs"
  | "batch"
  | "workflows"
  | "help"
  | "settings";

interface Props {
  activePage: AppPage;
  hasCurrentRun: boolean;
  runInProgress: boolean;
  isMac: boolean;
  dependenciesReady: boolean | null;
  dependenciesLoading: boolean;
  dependenciesError?: string | null;
  width: number;
  onResize: (width: number) => void;
  onNewRun: () => void;
  onNavigate: (page: AppPage) => void;
  onDependencies: () => void;
}

function Icon({ name }: { name: IconName }) {
  const common = {
    className: "h-[18px] w-[18px]",
    fill: "none",
    viewBox: "0 0 24 24",
    stroke: "currentColor",
    strokeWidth: 1.65,
  };

  switch (name) {
    case "new":
      return (
        <svg {...common}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M12 5v14M5 12h14" />
        </svg>
      );
    case "current":
      return (
        <svg {...common}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M12 6.75v5.25l3.5 2.25" />
          <circle cx="12" cy="12" r="8.25" />
        </svg>
      );
    case "runs":
      return (
        <svg {...common}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M7.5 4.75h9M7.5 9.5h9M7.5 14.25h5.5" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M5 3.25h14a1.5 1.5 0 0 1 1.5 1.5v14.5a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5V4.75A1.5 1.5 0 0 1 5 3.25Z" />
        </svg>
      );
    case "batch":
      return (
        <svg {...common}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M8 7h11M8 12h11M8 17h11" />
          <circle cx="4.5" cy="7" r=".75" fill="currentColor" stroke="none" />
          <circle cx="4.5" cy="12" r=".75" fill="currentColor" stroke="none" />
          <circle cx="4.5" cy="17" r=".75" fill="currentColor" stroke="none" />
        </svg>
      );
    case "workflows":
      return (
        <svg {...common}>
          <circle cx="6" cy="6" r="2.25" />
          <circle cx="18" cy="12" r="2.25" />
          <circle cx="6" cy="18" r="2.25" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M8.25 6h2.25A3.5 3.5 0 0 1 14 9.5v0A2.5 2.5 0 0 0 16.5 12M8.25 18h2.25A3.5 3.5 0 0 0 14 14.5v0A2.5 2.5 0 0 1 16.5 12" />
        </svg>
      );
    case "help":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="8.25" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M9.85 9.35a2.3 2.3 0 1 1 3.32 2.07c-.72.4-1.17.83-1.17 1.58v.25M12 16.75h.01" />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="2.75" />
          <path strokeLinecap="round" strokeLinejoin="round" d="M19 13.25v-2.5l-2.05-.52a5.7 5.7 0 0 0-.55-1.32l1.08-1.82-1.77-1.77-1.82 1.08a5.7 5.7 0 0 0-1.32-.55L12.05 3h-2.5l-.52 2.05a5.7 5.7 0 0 0-1.32.55L5.9 4.52 4.12 6.29 5.2 8.11a5.7 5.7 0 0 0-.55 1.32L2.6 9.95v2.5l2.05.52c.13.46.31.9.55 1.32l-1.08 1.82 1.77 1.77 1.82-1.08c.42.24.86.42 1.32.55l.52 2.05h2.5l.52-2.05c.46-.13.9-.31 1.32-.55l1.82 1.08 1.77-1.77-1.08-1.82c.24-.42.42-.86.55-1.32L19 13.25Z" />
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
}: {
  active: boolean;
  disabled?: boolean;
  icon: IconName;
  label: string;
  onClick: () => void;
  suffix?: ReactNode;
  title?: string;
}) {
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      disabled={disabled}
      onClick={onClick}
      title={title}
      className={`group flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-[13px] font-medium
                  transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400
                  disabled:cursor-not-allowed disabled:opacity-40 ${
                    active
                      ? "bg-white text-gray-950 shadow-sm ring-1 ring-gray-200/80 dark:bg-gray-800 dark:text-gray-50 dark:ring-gray-700"
                      : "text-gray-600 hover:bg-white/70 hover:text-gray-950 dark:text-gray-400 dark:hover:bg-gray-800/70 dark:hover:text-gray-100"
                  }`}
    >
      <span className={active ? "text-gray-900 dark:text-gray-100" : "text-gray-500 group-hover:text-gray-700 dark:text-gray-400 dark:group-hover:text-gray-200"}>
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
  isMac,
  dependenciesReady,
  dependenciesLoading,
  dependenciesError,
  width,
  onResize,
  onNewRun,
  onNavigate,
  onDependencies,
}: Props) {
  return (
    <aside
      style={{ width }}
      className={`relative flex shrink-0 flex-col border-r border-gray-200/80 bg-gray-100/90 px-3 pb-3
                  dark:border-gray-800 dark:bg-gray-950 ${isMac ? "pt-12" : "pt-4"}`}
    >
      {isMac && (
        <div
          data-tauri-drag-region
          className="absolute inset-x-0 top-0 h-12"
        />
      )}

      <div className="mb-5 px-2">
        <span className="text-base font-semibold tracking-[-0.01em] text-gray-900 dark:text-gray-100">
          Pipeline
        </span>
      </div>

      <nav aria-label="Primary" className="space-y-1">
        <RailButton
          active={activePage === "main" && !hasCurrentRun}
          disabled={runInProgress}
          icon="new"
          label="New run"
          onClick={onNewRun}
          title={runInProgress ? "A run is already in progress" : undefined}
        />
        {hasCurrentRun && (
          <RailButton
            active={activePage === "main"}
            icon="current"
            label="Current run"
            onClick={() => onNavigate("main")}
            suffix={runInProgress ? (
              <span
                aria-label="running"
                className="h-1.5 w-1.5 rounded-full bg-blue-500 shadow-[0_0_0_3px_rgba(59,130,246,0.12)]"
              />
            ) : undefined}
          />
        )}
        <RailButton
          active={activePage === "history"}
          icon="runs"
          label="History"
          onClick={() => onNavigate("history")}
        />
        <RailButton
          active={activePage === "batch"}
          icon="batch"
          label="Batch"
          onClick={() => onNavigate("batch")}
        />
        <RailButton
          active={activePage === "pipeline"}
          icon="workflows"
          label="Workflows"
          onClick={() => onNavigate("pipeline")}
        />
      </nav>

      <div className="mt-auto space-y-1 border-t border-gray-200/80 pt-3 dark:border-gray-800">
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
                     focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-gray-400 dark:hover:bg-gray-800/70
                     dark:hover:text-gray-200"
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
        defaultWidth={176}
        label="Resize primary navigation"
        min={152}
        max={320}
        onResize={onResize}
      />
    </aside>
  );
}
