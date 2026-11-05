import type { AppPage, RecentProject } from "./NavRail";
import { Icon } from "../ui/icons";

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
  runningReviews = reviewRunning ? 1 : 0,
  onAssistant,
  onNewProject,
  onOpenProject,
  onNewReview,
  onContinueReview,
  onNavigate,
  onSetUp,
  onExampleReview,
}: {
  projects: RecentProject[];
  projectsLoading: boolean;
  hasCurrentReview: boolean;
  reviewRunning: boolean;
  /** Reviews running right now (several can run at once). */
  runningReviews?: number;
  onAssistant: () => void;
  onNewProject: () => void;
  onOpenProject: (projectId: string) => void;
  onNewReview: () => void;
  onContinueReview: () => void;
  onNavigate: (page: AppPage) => void;
  /** Reopen the Get started guide. */
  onSetUp?: () => void;
  /** Open the bundled Example Review (no model call). */
  onExampleReview?: () => void;
}) {
  const actions = [
    {
      title: "Conversation",
      description:
        "Ask questions or get help with a draft, without setting up a project.",
      label: "New conversation",
      icon: "assistant" as const,
      accent:
        "from-blue-500/15 via-blue-500/5 to-transparent text-blue-700 dark:text-blue-300",
      onClick: onAssistant,
    },
    {
      title: "Review",
      description:
        "Review a paper and save the findings, supporting evidence, and report.",
      // Reviews run side by side, so this always sets up a new one; the
      // banner above returns to running reviews.
      label: "New review",
      icon: "review" as const,
      accent:
        "from-violet-500/15 via-violet-500/5 to-transparent text-violet-700 dark:text-violet-300",
      onClick: onNewReview,
    },
    {
      title: "Automate",
      description: "Chain tasks together in sequence or on a schedule.",
      label: "Open automations",
      icon: "tasks" as const,
      accent:
        "from-emerald-500/15 via-emerald-500/5 to-transparent text-emerald-700 dark:from-neutral-400/15 dark:via-neutral-400/5 dark:text-neutral-300",
      onClick: () => onNavigate("tasks"),
    },
  ];

  return (
    <div className="min-h-full bg-white text-gray-950 dark:bg-neutral-950 dark:text-neutral-50">
      <div className="mx-auto w-full max-w-6xl px-6 py-10 sm:px-10 lg:px-14 lg:py-14">
        {hasCurrentReview && (
          <button
            type="button"
            onClick={onContinueReview}
            className="flex w-full items-center gap-4 rounded-2xl border border-blue-200/80 bg-blue-50/70 px-5 py-4 text-left transition hover:border-blue-300 hover:bg-blue-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-400 dark:border-blue-900/80 dark:bg-blue-950/25 dark:hover:border-blue-800 dark:hover:bg-blue-950/40"
          >
            <span
              className={`h-2.5 w-2.5 shrink-0 rounded-full ${reviewRunning ? "animate-pulse bg-blue-500" : "bg-emerald-500"}`}
            />
            <span className="min-w-0 flex-1">
              <span className="block text-sm font-semibold">
                {runningReviews > 1
                  ? `${runningReviews} reviews are running`
                  : reviewRunning
                    ? "A review is running"
                    : "Your latest review is ready"}
              </span>
              <span className="mt-0.5 block text-xs text-gray-500 dark:text-neutral-400">
                {runningReviews > 1
                  ? "Open the earliest; Activity lists the rest"
                  : "Open review"}
              </span>
            </span>
            <Icon
              name="arrow"
              className="h-4 w-4 shrink-0 text-blue-600 dark:text-blue-300"
            />
          </button>
        )}

        <section
          aria-labelledby="start-heading"
          className={hasCurrentReview ? "mt-10" : undefined}
        >
          <h1 id="start-heading" className="text-base font-semibold">
            What would you like to do?
          </h1>
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
          {(onSetUp || onExampleReview) && (
            <p className="mt-4 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-gray-500 dark:text-neutral-400">
              <span>New to Pipeline?</span>
              {onSetUp && (
                <button
                  type="button"
                  onClick={onSetUp}
                  className="font-semibold text-gray-700 underline decoration-gray-300 underline-offset-2 hover:text-gray-950 hover:decoration-current focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-neutral-200 dark:decoration-neutral-600 dark:hover:text-neutral-50"
                >
                  Set up Pipeline
                </button>
              )}
              {onExampleReview && (
                <button
                  type="button"
                  onClick={onExampleReview}
                  className="font-semibold text-gray-700 underline decoration-gray-300 underline-offset-2 hover:text-gray-950 hover:decoration-current focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gray-400 dark:text-neutral-200 dark:decoration-neutral-600 dark:hover:text-neutral-50"
                >
                  See an example review
                </button>
              )}
            </p>
          )}
        </section>

        <div className="mt-12 max-w-3xl">
          <section aria-labelledby="projects-heading">
            <div className="flex items-center justify-between gap-4">
              <h2 id="projects-heading" className="text-base font-semibold">
                Recent projects
              </h2>
              <button
                type="button"
                onClick={() => onNavigate("project-index")}
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
        </div>
      </div>
    </div>
  );
}
