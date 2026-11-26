import type { ProjectHome } from "../src/lib/projectClient";
import type {
  Workspace,
  WorkbenchSession,
} from "../src/lib/workbench/sessionTypes";

/** Rich, disposable data for the real overview. Never connects to native IPC. */
export function overviewFixture(
  home: ProjectHome,
  workspace: Workspace,
  session: WorkbenchSession,
) {
  const at = (hours: number) =>
    new Date(Date.now() - hours * 3_600_000).toISOString();
  const record = <T>(id: string, kind: string, body: T) => ({
    id,
    kind,
    workspaceId: workspace.id,
    revision: 1,
    updatedAt: at(1),
    body,
  });
  workspace.name = "Local credit & firm investment";
  workspace.root = "/research/local-credit";
  session.title = "Robustness checks for referee 2";
  session.draft = "Compare the estimates with the leave-one-out sample.";
  session.updatedAt = at(0.5);
  home.settings.body.manuscriptRevisionId = home.papers[0].revision!.id;
  Object.assign(home.settings.body, {
    stage: "revising",
    kind: "empirical",
    venue: "Journal of Applied Economics",
    targetDate: new Date(Date.now() + 22 * 86400000).toISOString().slice(0, 10),
    targetLabel: "R1 due",
  });
  home.papers[0].paper.title = workspace.name;
  home.papers.forEach((p) => {
    p.revision!.capturedAt = at(1);
  });
  home.notes[0].body =
    "How does local credit supply shape firms’ investment after a monetary shock?";
  home.notes[1].kind = "assumption";
  home.notes[1].body = "Credit exposure is measured before the policy shock.";
  home.notes[1].updatedAt = at(1);
  home.tasks[0].body.objective = "Check the leave-one-out estimates";
  home.tasks.push(
    ...["Reconcile sample definitions", "Complete the response letter"].map(
      (objective, i) => ({
        ...home.tasks[0],
        id: `task-${i}`,
        body: { ...home.tasks[0].body, objective },
      }),
    ),
  );
  Object.assign(home, {
    changes: ["Clarify identification", "Update the Table 3 discussion"].map(
      (instruction, i) =>
        record(`edit-${i}`, "change", { state: "review", instruction }),
    ),
    workingCopyChanged: 3,
    workingCopyStatus: "3 source files changed since the current import",
    inventory: record("files", "inventory", {
      capturedAt: at(0.1),
      complete: true,
      rootIdentity: "fixture",
      warnings: [],
      files: ["paper.tex", "results.tex", "appendix.tex"].map((path) => ({
        path,
        hash: "updated",
        size: 1200,
        executable: false,
        status: "current",
      })),
    }),
  });
  localStorage.setItem(
    `pipeline.project.visit.${workspace.id}`,
    JSON.stringify({
      version: 1,
      lastActive: at(24),
      since: null,
      baseline: null,
      latest: { "paper.tex": "old" },
    }),
  );
  const repository = {
    branch: "revision/r1",
    head: "1111111",
    upstream: "origin/revision/r1",
    ahead: 1,
    behind: 0,
    fetchedAt: at(2),
    changed: 2,
    untracked: 0,
    conflicts: 0,
    changedPaths: ["paper.tex", "results.tex"],
    remote: {
      name: "origin",
      host: "github.com",
      owner: "example",
      repo: "local-credit",
      webUrl: "https://github.com/example/local-credit",
    },
    commits: [
      {
        sha: "1111111",
        author: "Researcher",
        date: at(3),
        subject: "Update revision estimates",
      },
    ],
    incoming: [],
  };
  const responses = Array.from({ length: 24 }, (_, i) =>
    record(`response-${i}`, "response", {
      decision: {
        number: `${i + 1}`,
        disposition: i < 18 ? "addressed" : i < 20 ? "investigating" : "open",
        draft: i < 21 ? "Reply prepared." : "",
        resolvingCheck: "",
        executionId: null,
      },
      flags: [],
    }),
  );
  const builds = [
    record("build", "build", {
      recordType: "receipt",
      outcome: "completed",
      diagnostics: [],
    }),
  ];
  const runs = [
    {
      run_id: "review",
      created: at(4),
      duration_secs: 60,
      input_path: `${workspace.root}/paper.tex`,
      input_name: "paper.tex",
      profile_name: "Full paper review",
      status: "done",
      failed_steps: [],
      title: "Referee preparation",
    },
  ];
  return (
    command: string,
    args: { kind?: string; request?: { action?: string } },
  ) => {
    if (command === "workbench_studio_records")
      return args.kind === "response"
        ? responses
        : args.kind === "build"
          ? builds
          : [];
    if (
      command === "workbench_repository_status" ||
      command === "workbench_repository_fetch"
    )
      return repository;
    if (command === "list_runs") return runs;
    if (command === "workbench_project_review_links") return [];
    if (command === "list_projects")
      return {
        projects: [
          { id: "reviews", name: "Project reviews", run_ids: ["review"] },
        ],
        warnings: [],
      };
    if (command === "sync_project_issue_ledger")
      return {
        issues: Array.from({ length: 5 }, (_, i) => ({
          id: `finding-${i}`,
          title: i ? "Clarify specification" : "Sample definition differs",
          severity: "high",
          status: i ? "open" : "regressed",
        })),
      };
    if (
      command === "workbench_project_mutate" &&
      ["refresh", "checkForChanges"].includes(args.request?.action ?? "")
    )
      return home;
    return undefined;
  };
}
