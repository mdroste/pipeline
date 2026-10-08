// Central route model for the app shell. The desktop app has no URL bar, but
// every destination still gets a typed route: navigation runs through one
// guarded channel, the current location survives relaunch, and routes
// serialize to the window hash so locations are copyable in development and
// traverse webview history (back/forward).

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
  | "gallery"
  | "activity";

export type WorkspaceSurface = "chat" | "project";

export type AppRoute =
  | { page: "home" }
  | { page: "main" }
  | { page: "project-index" }
  | {
      page: "workspace";
      /** Switch to this project on entry; omitted = last-used project. */
      projectId?: string;
      surface?: WorkspaceSurface;
      /** Project destination tab (see workspaceNavigation). */
      destination?: string;
      /** Open this conversation on entry. */
      sessionId?: string;
    }
  | {
      page: "tasks";
      taskId?: string;
      taskSessionId?: string;
      discoveryId?: string;
      /** Prefill the automation builder with this input file. */
      automatePath?: string;
    }
  | { page: "pipeline" }
  | { page: "gallery" }
  | { page: "batch" }
  | { page: "projects" }
  | { page: "history"; runId?: string }
  | { page: "settings"; section?: string; targetId?: string }
  | { page: "help"; section?: "privacy" }
  | { page: "activity" };

const PAGES: readonly AppPage[] = [
  "home",
  "main",
  "workspace",
  "project-index",
  "tasks",
  "pipeline",
  "settings",
  "help",
  "history",
  "batch",
  "projects",
  "gallery",
  "activity",
];

export function isAppPage(value: unknown): value is AppPage {
  return (
    typeof value === "string" && (PAGES as readonly string[]).includes(value)
  );
}

// Hash paths use product vocabulary; internal page ids stay compatibility
// names (see docs/tasks.md on naming contracts).
const PAGE_TO_PATH: Record<AppPage, string> = {
  home: "/home",
  main: "/reviews/new",
  "project-index": "/projects",
  workspace: "/project",
  tasks: "/automations",
  pipeline: "/reviews/designer",
  gallery: "/reviews/gallery",
  batch: "/reviews/batch",
  projects: "/reviews/collections",
  history: "/reviews/history",
  settings: "/settings",
  help: "/help",
  activity: "/activity",
};

const PATH_TO_PAGE = new Map<string, AppPage>(
  Object.entries(PAGE_TO_PATH).map(([page, path]) => [path, page as AppPage]),
);

export function serializeRoute(route: AppRoute): string {
  let path = PAGE_TO_PATH[route.page];
  const query = new URLSearchParams();
  switch (route.page) {
    case "workspace":
      if (route.projectId) path += `/${encodeURIComponent(route.projectId)}`;
      if (route.destination) query.set("dest", route.destination);
      if (route.surface) query.set("surface", route.surface);
      if (route.sessionId) query.set("session", route.sessionId);
      break;
    case "tasks":
      if (route.taskId) query.set("task", route.taskId);
      if (route.taskSessionId) query.set("session", route.taskSessionId);
      if (route.discoveryId) query.set("discovery", route.discoveryId);
      if (route.automatePath) query.set("automate", route.automatePath);
      break;
    case "history":
      if (route.runId) query.set("run", route.runId);
      break;
    case "settings":
      if (route.section) path += `/${encodeURIComponent(route.section)}`;
      if (route.targetId) query.set("target", route.targetId);
      break;
    case "help":
      if (route.section) query.set("section", route.section);
      break;
  }
  const suffix = query.toString();
  return `#${path}${suffix ? `?${suffix}` : ""}`;
}

/** Parse a serialized route ("#/reviews/history?run=…"). Unknown or malformed → null. */
export function parseRoute(
  serialized: string | null | undefined,
): AppRoute | null {
  if (!serialized) return null;
  const raw = serialized.startsWith("#") ? serialized.slice(1) : serialized;
  const [pathPart, queryPart] = raw.split("?", 2);
  const query = new URLSearchParams(queryPart ?? "");
  const segments = pathPart.split("/").filter(Boolean);
  if (segments.length === 0) return null;
  // Longest-prefix match so "/reviews/history" wins over "/reviews".
  for (let take = segments.length; take >= 1; take--) {
    const candidate = `/${segments.slice(0, take).join("/")}`;
    const page = PATH_TO_PAGE.get(candidate);
    if (!page) continue;
    let rest: string[];
    try {
      rest = segments.slice(take).map(decodeURIComponent);
    } catch {
      return null;
    }
    switch (page) {
      case "workspace":
        return {
          page,
          ...(rest[0] ? { projectId: rest[0] } : {}),
          ...(query.get("dest") ? { destination: query.get("dest")! } : {}),
          ...(query.get("surface") === "chat" ||
          query.get("surface") === "project"
            ? { surface: query.get("surface") as WorkspaceSurface }
            : {}),
          ...(query.get("session") ? { sessionId: query.get("session")! } : {}),
        };
      case "tasks":
        return {
          page,
          ...(query.get("task") ? { taskId: query.get("task")! } : {}),
          ...(query.get("session")
            ? { taskSessionId: query.get("session")! }
            : {}),
          ...(query.get("discovery")
            ? { discoveryId: query.get("discovery")! }
            : {}),
          ...(query.get("automate")
            ? { automatePath: query.get("automate")! }
            : {}),
        };
      case "history":
        return {
          page,
          ...(query.get("run") ? { runId: query.get("run")! } : {}),
        };
      case "settings":
        return {
          page,
          ...(rest[0] ? { section: rest[0] } : {}),
          ...(query.get("target") ? { targetId: query.get("target")! } : {}),
        };
      case "help":
        return {
          page,
          ...(query.get("section") === "privacy"
            ? { section: "privacy" as const }
            : {}),
        };
      default:
        return { page };
    }
  }
  return null;
}

export interface RouterSnapshot {
  route: AppRoute;
  /** Increments on every commit, including re-commits of an equal route, so
   * subscribers can honor repeated entry requests ("open project" twice). */
  revision: number;
}

export type NavigationGuard = (next: AppRoute) => Promise<boolean>;

const ROUTE_STORAGE_KEY = "pipeline.ui.route";

class Router {
  private snapshot: RouterSnapshot = { route: { page: "home" }, revision: 0 };
  private listeners = new Set<(snapshot: RouterSnapshot) => void>();
  private guard: NavigationGuard | null = null;
  private hashInstalled = false;
  private writingHash = false;

  /** Set the initial route without persisting or guarding (startup only). */
  init(route: AppRoute) {
    this.snapshot = { route, revision: this.snapshot.revision + 1 };
  }

  getSnapshot = (): RouterSnapshot => this.snapshot;

  get route(): AppRoute {
    return this.snapshot.route;
  }

  subscribe = (listener: (snapshot: RouterSnapshot) => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  setGuard(guard: NavigationGuard | null) {
    this.guard = guard;
  }

  /** Guarded navigation. Resolves false when the guard declined. */
  async navigate(next: AppRoute): Promise<boolean> {
    if (this.guard && !(await this.guard(next))) return false;
    this.apply(next);
    return true;
  }

  /** Commit a route without running the guard (for flows that already asked). */
  apply(next: AppRoute) {
    const previous = serializeRoute(this.snapshot.route);
    this.snapshot = { route: next, revision: this.snapshot.revision + 1 };
    try {
      localStorage.setItem(ROUTE_STORAGE_KEY, serializeRoute(next));
    } catch {
      /* Preference storage is optional. */
    }
    this.syncHash(previous);
    for (const listener of [...this.listeners]) listener(this.snapshot);
  }

  /** Reflect committed routes into the window hash and honor back/forward. */
  installHashSync() {
    if (this.hashInstalled || typeof window === "undefined") return;
    this.hashInstalled = true;
    window.addEventListener("hashchange", () => {
      if (this.writingHash) return;
      const current = serializeRoute(this.snapshot.route);
      if (window.location.hash === current) return;
      const parsed = parseRoute(window.location.hash);
      if (!parsed) {
        this.writeHash(current, true);
        return;
      }
      void (this.guard ? this.guard(parsed) : Promise.resolve(true)).then(
        (allowed) => {
          if (allowed) this.apply(parsed);
          else this.writeHash(current, true);
        },
      );
    });
    // Adopt a pre-set hash (dev deep link) or publish the current route.
    const initial = parseRoute(window.location.hash);
    if (initial) this.apply(initial);
    else this.writeHash(serializeRoute(this.snapshot.route), true);
  }

  private syncHash(previousSerialized: string) {
    if (!this.hashInstalled) return;
    const next = serializeRoute(this.snapshot.route);
    if (next === previousSerialized && window.location.hash === next) return;
    this.writeHash(next, next === previousSerialized);
  }

  private writeHash(hash: string, replace: boolean) {
    this.writingHash = true;
    try {
      if (replace) {
        window.history.replaceState(null, "", hash);
      } else {
        window.location.hash = hash;
      }
    } catch {
      /* History access is optional (some webviews restrict it). */
    } finally {
      // hashchange fires asynchronously; release the flag in a microtask so
      // our own write is ignored but user navigation is not.
      void Promise.resolve().then(() => {
        this.writingHash = false;
      });
    }
  }
}

export const router = new Router();

/** The route a fresh launch should restore, from the persisted location. */
export function restoreRoute(
  storage: Pick<Storage, "getItem">,
): AppRoute | null {
  const read = (key: string): string | null => {
    try {
      return storage.getItem(key);
    } catch {
      // A blocked preference store must not prevent startup.
      return null;
    }
  };
  const parsed = parseRoute(read(ROUTE_STORAGE_KEY));
  if (parsed) return parsed;
  // Legacy key from builds that only remembered four pages.
  const legacy = read("pipeline.ui.page");
  if (legacy === "workspace" || legacy === "project-index" || legacy === "home")
    return { page: legacy };
  if (legacy === "main") return { page: "main" };
  return null;
}
