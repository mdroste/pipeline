import type { AppPage, RecentProject } from "./NavRail";

type HomeIcon =
  | "assistant"
  | "review"
  | "plan"
  | "project"
  | "history"
  | "designer"
  | "arrow";

function Icon({
  name,
  className = "h-5 w-5",
}: {
  name: HomeIcon;
  className?: string;
}) {
  const common = {
    className,
    fill: "none",
    viewBox: "0 0 24 24",
    stroke: "currentColor",
    strokeWidth: 1.65,
    "aria-hidden": true as const,
  };
  if (name === "assistant")
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
  if (name === "review")
    return (
      <svg {...common}>
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M14 3.5H6A1.5 1.5 0 0 0 4.5 5v14A1.5 1.5 0 0 0 6 20.5h12a1.5 1.5 0 0 0 1.5-1.5V9L14 3.5ZM14 3.5V9h5.5M8 14l2.5 2.5L16 11"
        />
      </svg>
    );
  if (name === "plan")
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
  if (name === "project")
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
          d="M5.75 7.25V5.5A1.5 1.5 0 0 1 7.25 4h4.25l1.5 2h4.75a1.5 1.5 0 0 1 1.5 1.5v1.75"
        />
      </svg>
    );
  if (name === "history")
    return (
      <svg {...common}>
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M3.5 4.5v5h5M3.8 9a8.25 8.25 0 1 1 .3 6M12 7.5V12l3 2"
        />
      </svg>
    );
  if (name === "designer")
    return (
      <svg {...common}>
        <circle cx="6" cy="6" r="2.25" />
        <circle cx="18" cy="12" r="2.25" />
        <circle cx="6" cy="18" r="2.25" />
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M8.25 6h2.25A3.5 3.5 0 0 1 14 9.5 2.5 2.5 0 0 0 16.5 12M8.25 18h2.25A3.5 3.5 0 0 0 14 14.5 2.5 2.5 0 0 1 16.5 12"
        />
      </svg>
    );
  return (
    <svg {...common}>
      <path
        strokeLinecap="round"
        strokeLinejoin="round"
        d="M5 12h14m-5-5 5 5-5 5"
      />
    </svg>
  );
}

function projectLocation(project: RecentProject) {
  if (project.missingRootAt) return "Folder unavailable";
  if (!project.root) return "No folder attached";
  return (
    project.root
      .replace(/[\\/]+$/, "")
      .split(/[\\/]/)
      .pop() || project.root
  );
}

export default function HomePage({
  projects,
  projectsLoading,
  hasCurrentReview,
  reviewRunning,
  onAssistant,
  onNewProject,
  onOpenProject,
  onNewReview,
  onContinueReview,
  onNavigate,
}: {
  projects: RecentProject[];
  projectsLoading: boolean;
  hasCurrentReview: boolean;
  reviewRunning: boolean;
  onAssistant: () => void;
  onNewProject: () => void;
  onOpenProject: (projectId: string) => void;
  onNewReview: () => void;
  onContinueReview: () => void;
  onNavigate: (page: AppPage) => void;
}) {
  const actions = [
    {
      title: "Work with the assistant",
      description:
        "Develop an argument, revise prose, or investigate results in a project.",
      label: "Open assistant",
      icon: "assistant" as const,
      accent:
        "from-blue-500/15 via-blue-500/5 to-transparent text-blue-700 dark:text-blue-300",
      onClick: onAssistant,
    },
    {
      title: "Review a paper",
      description:
        "Run a structured, reproducible review with evidence and durable outputs.",
      label: reviewRunning ? "Continue review" : "New review",
      icon: "review" as const,
      accent:
        "from-violet-500/15 via-violet-500/5 to-transparent text-violet-700 dark:text-violet-300",
      onClick: reviewRunning ? onContinueReview : onNewReview,
    },
    {
      title: "Automate research",
      description:
        "Continue research, reviews, waits, and decisions on a schedule or over several rounds.",
      label: "Open automations",
      icon: "plan" as const,
      accent:
        "from-emerald-500/15 via-emerald-500/5 to-transparent text-emerald-700 dark:text-emerald-300",
      onClick: () => onNavigate("tasks"),
    },
  ];

  return (
    <div className="min-h-full bg-white text-gray-950 dark:bg-neutral-950 dark:text-neutral-50">
      <div className="mx-auto w-full max-w-6xl px-6 py-10 sm:px-10 lg:px-14 lg:py-14">
        <header className="max-w-3xl">
          <p className="text-xs font-semibold uppercase tracking-[0.14em] text-gray-400 dark:text-neutral-500">
            Research home
          </p>
          <h1 className="mt-3 text-3xl font-semibold tracking-[-0.035em] sm:text-4xl">
            Move your research forward.
          </h1>
          <p className="mt-3 max-w-2xl text-[15px] leading-7 text-gray-500 dark:text-neutral-400">
            Work in a project, ask the assistant, run a rigorous review, or
            continue a research automation.
          </p>
        </header>

        {hasCurrentReview && (
          <button
            type="button"
            onClick={onContinueReview}
            className="mt-8 flex w-full items-center gap-4 rounded-2xl border border-blue-200/80 bg-blue-50/70 px-5 py-4 text-left transition hover:border-blue-300 hover:bg-blue-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 dark:border-blue-900/80 dark:bg-blue-950/25 dark:hover:border-blue-800 dark:hover:bg-blue-950/40"
          >
            <span
              className={`h-2.5 w-2.5 shrink-0 rounded-full ${reviewRunning ? "animate-pulse bg-blue-500" : "bg-emerald-500"}`}
            />
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-semibold">
                {reviewRunning
                  ? "A review is running"
                  : "Your latest review is ready"}
              </span>
              <span className="mt-0.5 block text-xs text-gray-500 dark:text-neutral-400">
                Open the current review
              </span>
            </span>
            <Icon
              name="arrow"
              className="h-4 w-4 shrink-0 text-blue-600 dark:text-blue-300"
            />
          </button>
        )}

        <section aria-labelledby="start-heading" className="mt-10">
          <div className="flex items-end justify-between gap-4">
            <div>
              <h2 id="start-heading" className="text-base font-semibold">
                What would you like to do?
              </h2>
              <p className="mt-1 text-sm text-gray-500 dark:text-neutral-400">
                Start with the outcome; Pipeline will open the right place.
              </p>
            </div>
          </div>
          <div className="mt-4 grid gap-4 md:grid-cols-3">
            {actions.map((action) => (
              <button
                key={action.title}
                type="button"
                onClick={action.onClick}
                className="group relative min-h-52 overflow-hidden rounded-2xl border border-gray-200 bg-white p-5 text-left shadow-[0_1px_2px_rgba(0,0,0,0.03)] transition duration-200 hover:-translate-y-0.5 hover:border-gray-300 hover:shadow-[0_12px_30px_rgba(0,0,0,0.08)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:border-neutral-800 dark:bg-neutral-900/70 dark:hover:border-neutral-700 dark:hover:shadow-black/30"
              >
                <span
                  className={`absolute inset-0 bg-gradient-to-br ${action.accent}`}
                  aria-hidden="true"
                />
                <span className="relative flex h-full flex-col">
                  <span className="flex h-10 w-10 items-center justify-center rounded-xl border border-current/15 bg-white/75 shadow-sm backdrop-blur dark:bg-neutral-950/60">
                    <Icon name={action.icon} />
                  </span>
                  <span className="mt-8 block text-base font-semibold tracking-[-0.01em] text-gray-950 dark:text-neutral-50">
                    {action.title}
                  </span>
                  <span className="mt-2 block text-sm leading-6 text-gray-500 dark:text-neutral-400">
                    {action.description}
                  </span>
                  <span className="mt-auto flex items-center gap-2 pt-5 text-xs font-semibold text-gray-700 dark:text-neutral-200">
                    {action.label}
                    <Icon
                      name="arrow"
                      className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5"
                    />
                  </span>
                </span>
              </button>
            ))}
          </div>
        </section>

        <div className="mt-12 grid gap-8 lg:grid-cols-[minmax(0,1.45fr)_minmax(260px,0.75fr)]">
          <section aria-labelledby="projects-heading">
            <div className="flex items-center justify-between gap-4">
              <div>
                <h2 id="projects-heading" className="text-base font-semibold">
                  Continue working
                </h2>
                <p className="mt-1 text-sm text-gray-500 dark:text-neutral-400">
                  Your recently updated research projects.
                </p>
              </div>
              <button
                type="button"
                onClick={() => onNavigate("workspace")}
                className="rounded-lg px-2.5 py-1.5 text-xs font-semibold text-gray-600 transition hover:bg-gray-100 hover:text-gray-950 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
              >
                View all
              </button>
            </div>
            <div className="mt-4 overflow-hidden rounded-2xl border border-gray-200 dark:border-neutral-800">
              {projects.slice(0, 4).map((project, index) => (
                <button
                  key={project.id}
                  type="button"
                  onClick={() => onOpenProject(project.id)}
                  className={`group flex w-full items-center gap-4 px-4 py-4 text-left transition hover:bg-gray-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-gray-400 dark:hover:bg-neutral-900 ${index ? "border-t border-gray-200 dark:border-neutral-800" : ""}`}
                >
                  <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl bg-gray-100 text-gray-600 dark:bg-neutral-800 dark:text-neutral-300">
                    <Icon name="project" />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-semibold">
                      {project.name}
                    </span>
                    <span
                      className={`mt-1 block truncate text-xs ${project.missingRootAt ? "text-amber-600 dark:text-amber-400" : "text-gray-500 dark:text-neutral-500"}`}
                    >
                      {projectLocation(project)}
                    </span>
                  </span>
                  <Icon
                    name="arrow"
                    className="h-4 w-4 shrink-0 text-gray-300 transition group-hover:translate-x-0.5 group-hover:text-gray-500 dark:text-neutral-700 dark:group-hover:text-neutral-400"
                  />
                </button>
              ))}
              {!projects.length && (
                <div className="px-5 py-8 text-center">
                  <div className="mx-auto flex h-11 w-11 items-center justify-center rounded-xl bg-gray-100 text-gray-500 dark:bg-neutral-800 dark:text-neutral-400">
                    <Icon name="project" />
                  </div>
                  <p className="mt-3 text-sm font-semibold">
                    {projectsLoading
                      ? "Loading projects…"
                      : "Start your first project"}
                  </p>
                  <p className="mt-1 text-xs leading-5 text-gray-500 dark:text-neutral-500">
                    Projects keep conversations, files, and research decisions
                    together.
                  </p>
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={onNewProject}
              className="mt-3 inline-flex items-center gap-2 rounded-lg border border-gray-200 bg-white px-3 py-2 text-xs font-semibold text-gray-700 transition hover:border-gray-300 hover:bg-gray-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:border-neutral-800 dark:bg-neutral-900 dark:text-neutral-300 dark:hover:border-neutral-700 dark:hover:bg-neutral-800"
            >
              <span aria-hidden="true" className="text-base leading-none">
                +
              </span>{" "}
              New project
            </button>
          </section>

          <section aria-labelledby="tools-heading">
            <h2 id="tools-heading" className="text-base font-semibold">
              Tools and history
            </h2>
            <p className="mt-1 text-sm text-gray-500 dark:text-neutral-400">
              Direct access for repeatable and advanced work.
            </p>
            <div className="mt-4 rounded-2xl border border-gray-200 p-2 dark:border-neutral-800">
              {[
                [
                  "Review history",
                  "Completed and interrupted runs",
                  "history",
                  "history",
                ],
                [
                  "Review collections",
                  "Related reports and issue ledgers",
                  "project",
                  "projects",
                ],
                [
                  "Review designer",
                  "Review steps, prompts, and output schemas",
                  "designer",
                  "pipeline",
                ],
              ].map(([title, description, icon, page]) => (
                <button
                  key={title}
                  type="button"
                  onClick={() => onNavigate(page as AppPage)}
                  className="group flex w-full items-center gap-3 rounded-xl px-3 py-3 text-left transition hover:bg-gray-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:hover:bg-neutral-900"
                >
                  <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-gray-100 text-gray-600 dark:bg-neutral-800 dark:text-neutral-300">
                    <Icon
                      name={icon as HomeIcon}
                      className="h-[18px] w-[18px]"
                    />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block text-sm font-medium">{title}</span>
                    <span className="mt-0.5 block truncate text-xs text-gray-500 dark:text-neutral-500">
                      {description}
                    </span>
                  </span>
                  <Icon
                    name="arrow"
                    className="h-3.5 w-3.5 text-gray-300 transition group-hover:translate-x-0.5 dark:text-neutral-700"
                  />
                </button>
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
