import { beforeEach, describe, expect, it } from "vitest";
import type { ProjectHome } from "./projectClient";
import type { PaperWithRevision, WorkbenchSession } from "./workbenchTypes";
import type { ProjectIssue, RunSummary } from "./types";
import type { RepositoryStatus } from "./repositoryClient";
import {
  NO_SIGNALS,
  VISIT_GAP_MS,
  briefNotes,
  currentManuscript,
  diffFiles,
  draftBlock,
  isThinProject,
  leftOff,
  recordVisit,
  repositoryBlock,
  roundBlock,
  sinceBlock,
  syncBlock,
  targetSummary,
  underRoot,
  type OverviewBlock,
  type ProjectSignals,
} from "./projectOverview";

function home(patch: Partial<ProjectHome> = {}): ProjectHome {
  return {
    settings: {
      id: "home",
      workspaceId: "ws",
      kind: "home",
      revision: 1,
      updatedAt: "2026-09-01T00:00:00Z",
      body: {
        manuscriptRevisionId: null,
        baselineExecutionId: null,
        briefNoteIds: [],
        excludedNoteIds: [],
        ignoredPaths: [],
        layout: "",
      },
    },
    notes: [],
    noteHistory: [],
    tasks: [],
    anchors: [],
    papers: [],
    executions: [],
    ledger: {
      claims: [],
      evidence: [],
      staleClaims: [],
      unsupportedAcceptedClaims: [],
    },
    inventory: null,
    changes: [],
    applications: [],
    contextPreview: "",
    workingCopyStatus: "",
    fileAcceptance: true,
    ...patch,
  };
}
const paper = (id: string, capturedAt: string): PaperWithRevision => ({
  paper: {
    id: `paper-${id}`,
    workspaceId: "ws",
    title: "Minimum wage",
    role: "manuscript",
    currentRevisionId: id,
    createdAt: capturedAt,
    updatedAt: capturedAt,
  },
  revision: {
    id,
    paperId: `paper-${id}`,
    inputKind: "file",
    entrypoint: "/research/paper.tex",
    dependencyManifest: {},
    contentHash: `hash-${id}`,
    textReference: null,
    compiledArtifactId: null,
    extraction: {},
    captureComplete: true,
    capturedAt,
  },
});
const withManuscript = (capturedAt = "2026-09-20T10:00:00Z") => {
  const data = home({ papers: [paper("rev", capturedAt)] });
  data.settings.body.manuscriptRevisionId = "rev";
  return data;
};
const reviewRun = (patch: Partial<RunSummary> = {}): RunSummary => ({
  run_id: "run",
  created: "2026-09-10T10:00:00Z",
  input_name: "paper.tex",
  input_path: "/research/paper.tex",
  input_mode: "document",
  profile_id: "review",
  profile_name: "Paper review",
  provider: "claude",
  status: "complete",
  duration_secs: 600,
  input_tokens: 0,
  output_tokens: 0,
  cached_input_tokens: 0,
  cache_write_input_tokens: 0,
  step_count: 4,
  artifact_count: 4,
  failed_steps: [],
  resumable: false,
  title: "Draft 7",
  tags: [],
  ...patch,
});
const issue = (
  id: string,
  status: ProjectIssue["status"],
  severity = "medium",
): ProjectIssue => ({
  id,
  title: `Issue ${id}`,
  severity,
  section: "",
  status,
  note: "",
  created: "",
  updated: "",
  decision_updated: "",
  occurrences: [],
});
const signals = (patch: Partial<ProjectSignals>): ProjectSignals => ({
  ...NO_SIGNALS,
  ...patch,
});
const row = (block: OverviewBlock | null, id: string) =>
  block?.rows.find((item) => item.id === id);

beforeEach(() => localStorage.clear());

describe("silence and single actions", () => {
  it("renders nothing for a project with nothing to derive", () => {
    const data = home();
    expect(draftBlock(data, NO_SIGNALS)).toBeNull();
    expect(syncBlock(data, NO_SIGNALS)).toBeNull();
    expect(roundBlock(NO_SIGNALS)).toBeNull();
    expect(
      sinceBlock(data, NO_SIGNALS, { since: null, baseline: null }),
    ).toBeNull();
    expect(isThinProject(data, null)).toBe(true);
  });

  it("is not thin once a folder or a paper exists", () => {
    const workspace = { root: "/research" } as never;
    expect(isThinProject(home(), workspace)).toBe(false);
    expect(isThinProject(withManuscript(), null)).toBe(false);
  });

  it("scopes review runs to the attached folder", () => {
    expect(underRoot("/research/paper.tex", "/research/")).toBe(true);
    expect(underRoot("/research-old/paper.tex", "/research")).toBe(false);
  });
});

describe("state of the draft", () => {
  it("reports the manuscript and that it was never reviewed", () => {
    const block = draftBlock(withManuscript(), signals({ runs: [] }));
    expect(row(block, "manuscript")?.action).toEqual({
      kind: "document",
      label: "Open",
      revisionId: "rev",
    });
    expect(row(block, "review")).toMatchObject({
      text: "This draft has not been reviewed",
      action: { kind: "review" },
    });
  });

  it("asks for a current-version choice when none was made", () => {
    const data = home({ papers: [paper("rev", "2026-09-20T10:00:00Z")] });
    expect(currentManuscript(data).chosen).toBe(false);
    expect(row(draftBlock(data, NO_SIGNALS), "manuscript")?.action).toEqual({
      kind: "tab",
      label: "Choose the current version",
      tab: "project-settings",
    });
  });

  it("flags a version imported after the last review", () => {
    const block = draftBlock(
      withManuscript("2026-09-20T10:00:00Z"),
      signals({ runs: [reviewRun({ created: "2026-09-10T10:00:00Z" })] }),
    );
    expect(row(block, "review")).toMatchObject({
      text: "The current version has not been reviewed",
      attention: true,
      action: { kind: "review" },
    });
  });

  it("links the latest report when it covers the current version", () => {
    const block = draftBlock(
      withManuscript("2026-09-01T10:00:00Z"),
      signals({
        runs: [
          reviewRun({ run_id: "old", created: "2026-09-05T10:00:00Z" }),
          reviewRun({ run_id: "new", created: "2026-09-12T10:00:00Z" }),
        ],
      }),
    );
    expect(row(block, "review")?.action).toEqual({
      kind: "run",
      label: "Open report",
      runId: "new",
    });
  });

  it("counts outstanding findings and leads with regressions", () => {
    const block = draftBlock(
      withManuscript(),
      signals({
        ledgers: [
          {
            schema_version: 1,
            project_id: "collection",
            updated: "",
            warnings: [],
            issues: [
              issue("a", "open", "low"),
              issue("b", "regressed", "low"),
              issue("c", "open", "high"),
              issue("d", "addressed"),
              { ...issue("e", "open"), archived: true },
            ],
          },
        ],
      }),
    );
    const findings = row(block, "findings");
    expect(findings?.text).toBe("3 review findings outstanding, 1 regressed");
    expect(findings?.detail).toContain("1 addressed");
    expect(findings?.detail).toContain("most pressing: Issue b");
    expect(findings?.attention).toBe(true);
  });

  it("offers the assistant a failed build with its diagnostics", () => {
    const block = draftBlock(
      withManuscript(),
      signals({
        builds: [
          {
            id: "receipt",
            workspaceId: "ws",
            kind: "build",
            revision: 1,
            updatedAt: "2026-09-21T10:00:00Z",
            body: {
              recordType: "receipt",
              executionId: "exec",
              outcome: "failed",
              diagnostics: [
                {
                  severity: "error",
                  message: "Undefined control sequence",
                  path: "paper.tex",
                  line: 42,
                },
              ],
              pdf: null,
              mappingArtifactId: null,
              pageInspection: "",
              synchronization: "",
              sourceManifest: null,
            },
          },
        ],
      }),
    );
    const build = row(block, "build");
    expect(build?.text).toBe("The last build failed");
    expect(build?.action.kind).toBe("ask");
    expect(build?.action).toMatchObject({
      prompt: expect.stringContaining(
        "Undefined control sequence (paper.tex:42)",
      ),
    });
  });
});

describe("out of sync", () => {
  it("surfaces stale numbers, claims, changed files, and change impact", () => {
    const data = withManuscript();
    data.workingCopyChanged = 2;
    data.ledger.staleClaims = ["claim"];
    const block = syncBlock(
      data,
      signals({
        coverage: [
          {
            state: "stale",
            reasons: ["Declared execution inputs changed"],
            record: { body: { printed: "0.042" } },
          },
          { state: "current", reasons: [], record: { body: { printed: "1" } } },
          { state: "unknown", reasons: [], record: { body: { printed: "2" } } },
        ] as never,
        impact: {
          impacts: [
            { status: "input_changed", reason: "Input changed", path: [{}] },
            {
              status: "review_needed",
              reason: "Input changed via binding",
              path: [{}, {}],
            },
          ],
          relations: [],
          complete: true,
          limitations: [],
        } as never,
      }),
    );
    expect(block?.rows.map((item) => item.id)).toEqual([
      "working-copy",
      "numbers",
      "numbers-unverified",
      "stale-claims",
      "impact",
    ]);
    expect(row(block, "numbers")?.text).toBe(
      "1 number in the draft is out of date with its results",
    );
    expect(row(block, "numbers")?.detail).toContain("“0.042”");
    expect(row(block, "impact")?.text).toBe(
      "1 linked item needs rechecking after 1 upstream change",
    );
    for (const item of block!.rows) expect(item.action.label).toBeTruthy();
  });

  it("notices an earlier version still marked current", () => {
    const data = home({ papers: [paper("new", "2026-09-20T10:00:00Z")] });
    data.settings.body.manuscriptRevisionId = "old";
    expect(row(syncBlock(data, NO_SIGNALS), "superseded")).toBeDefined();
  });
});

describe("revision round", () => {
  const response = (
    id: string,
    disposition: string,
    draft = "",
    resolvingCheck = "",
  ) =>
    ({
      id,
      body: {
        source: { kind: "report" },
        flags: [],
        decision: {
          number: id,
          disposition,
          draft,
          resolvingCheck,
          executionId: null,
        },
      },
    }) as never;

  it("appears only when review comments exist", () => {
    expect(roundBlock(signals({ responses: [] }))).toBeNull();
  });

  it("counts progress, undrafted replies, and unrun checks", () => {
    const block = roundBlock(
      signals({
        responses: [
          response("R1.1", "addressed", "Done"),
          response("R1.2", "open"),
          response(
            "R1.3",
            "investigating",
            "Draft",
            "Re-run with state trends",
          ),
          response("R2.1", "rejected"),
        ],
      }),
    );
    expect(row(block, "progress")).toMatchObject({
      text: "1 of 4 review comments addressed",
      detail: "1 in progress · 1 open · 1 rejected",
    });
    expect(row(block, "undrafted")).toMatchObject({
      text: "1 comment has no drafted reply",
      action: { kind: "ask", prompt: expect.stringContaining("R1.2") },
    });
    expect(row(block, "waiting")?.detail).toBe("Re-run with state trends");
  });
});

describe("visits", () => {
  const file = (path: string, hash: string) => ({
    path,
    hash,
    size: 1,
    status: "current",
    executable: false,
  });

  it("keeps one baseline for a whole visit and starts a new one after a gap", () => {
    const start = new Date("2026-09-01T09:00:00Z");
    expect(recordVisit("ws", start, [file("a.do", "1")])).toEqual({
      since: null,
      baseline: null,
    });
    const later = new Date(start.getTime() + 10 * 60_000);
    expect(recordVisit("ws", later, [file("a.do", "2")]).since).toBeNull();

    const next = new Date(later.getTime() + VISIT_GAP_MS + 1);
    const visit = recordVisit("ws", next, [file("a.do", "3")]);
    expect(visit.since).toBe(later.toISOString());
    expect(visit.baseline).toEqual({ "a.do": "2" });
    // Activity inside the new visit does not move its baseline.
    const again = recordVisit("ws", new Date(next.getTime() + 60_000), [
      file("a.do", "4"),
    ]);
    expect(again).toEqual(visit);
  });

  it("starts over from a corrupt marker", () => {
    localStorage.setItem("pipeline.project.visit.ws", "{not json");
    expect(recordVisit("ws", new Date(), null).since).toBeNull();
  });

  it("diffs changed, added, and removed files", () => {
    expect(diffFiles({ a: "1", b: "1" }, { a: "2", c: "1" })).toEqual({
      changed: ["a"],
      added: ["c"],
      removed: ["b"],
    });
  });

  it("lists what changed while the researcher was away", () => {
    const data = withManuscript("2026-09-02T08:00:00Z");
    data.inventory = {
      id: "inventory",
      workspaceId: "ws",
      kind: "inventory",
      revision: 1,
      updatedAt: "",
      body: {
        rootIdentity: "root",
        capturedAt: "",
        complete: true,
        warnings: [],
        files: [file("analysis/main.do", "new"), file("paper.tex", "same")],
      },
    };
    const block = sinceBlock(
      data,
      signals({
        runs: [
          reviewRun({ run_id: "before", created: "2026-08-30T10:00:00Z" }),
          reviewRun({ run_id: "after", created: "2026-09-01T23:00:00Z" }),
        ],
        automations: [
          {
            id: "task",
            revision: 1,
            name: "Re-review loop",
            state: "failed",
            reason: "Provider limit",
            createdAt: 0,
            updatedAt: Date.parse("2026-09-02T01:00:00Z") / 1000,
            dueAt: null,
            sessionId: "s",
            scheduleId: null,
          },
        ],
      }),
      {
        since: "2026-09-01T12:00:00Z",
        baseline: { "analysis/main.do": "old", "paper.tex": "same" },
      },
    );
    expect(block?.rows.map((item) => item.id)).toEqual([
      "files",
      "version-rev",
      "review-after",
      "automation-task",
    ]);
    expect(row(block, "files")?.detail).toBe("analysis/main.do");
    expect(row(block, "automation-task")).toMatchObject({
      text: "Automation failed: Re-review loop",
      attention: true,
      action: { kind: "automation", taskId: "task" },
    });
  });
});

describe("where you left off", () => {
  const session = (
    id: string,
    updatedAt: string,
    patch: Partial<WorkbenchSession> = {},
  ): WorkbenchSession => ({
    id,
    workspaceId: "ws",
    paperId: null,
    title: id,
    presetId: null,
    overrides: {},
    draft: "",
    revision: 1,
    archivedAt: null,
    createdAt: updatedAt,
    updatedAt,
    ...patch,
  });

  it("recalls the unsent draft, last request, and next steps of the latest conversation", () => {
    const data = home({
      notes: [
        {
          id: "next",
          workspaceId: "ws",
          paperId: null,
          kind: "next_step",
          body: "Re-estimate with county trends",
          state: "accepted",
          origin: "user",
          pinned: false,
          revision: 1,
          createdAt: "",
          updatedAt: "2026-09-02",
        },
      ],
      executions: [
        { sessionId: "latest", outcome: "failed" },
        { sessionId: "latest", outcome: "completed" },
        { sessionId: "old", outcome: "completed" },
      ] as never,
    });
    const result = leftOff(
      data,
      [
        session("old", "2026-09-01"),
        session("latest", "2026-09-03", { draft: "  And the pre-trends?  " }),
        session("archived", "2026-09-09", { archivedAt: "2026-09-09" }),
        session("elsewhere", "2026-09-09", { workspaceId: "other" }),
      ],
      "ws",
      {
        sessionId: "latest",
        items: [
          { itemKind: "userMessage", payload: { text: "First" } },
          { itemKind: "agentMessage", payload: { text: "Answer" } },
          { itemKind: "userMessage", payload: { text: "Does Table 4 hold?" } },
        ] as never,
      },
      (item) => String(item.payload?.text ?? ""),
    );
    expect(result).toMatchObject({
      session: { id: "latest" },
      draft: "And the pre-trends?",
      lastAsk: "Does Table 4 hold?",
      runs: { total: 2, failed: 1 },
    });
    expect(result?.nextSteps.map((note) => note.id)).toEqual(["next"]);
  });

  it("is absent without a conversation", () => {
    expect(leftOff(home(), [], "ws", null, () => "")).toBeNull();
  });
});

describe("brief and target", () => {
  it("never puts a proposed note in the brief", () => {
    const data = home({
      notes: [
        {
          id: "proposed",
          workspaceId: "ws",
          paperId: null,
          kind: "question",
          body: "Unaccepted",
          state: "proposed",
          origin: "assistant",
          pinned: true,
          revision: 1,
          createdAt: "",
          updatedAt: "",
        },
      ],
    });
    data.settings.body.briefNoteIds = ["proposed"];
    expect(briefNotes(data)).toEqual([]);
  });

  it("counts days to an optional target date", () => {
    const now = new Date(2026, 9, 1, 15);
    expect(targetSummary({}, now)).toBeNull();
    expect(
      targetSummary({ targetDate: "2026-10-15", targetLabel: " " }, now),
    ).toMatchObject({ label: "Target", days: 14 });
    expect(
      targetSummary({ targetDate: "2026-09-30", targetLabel: "R&R" }, now),
    ).toMatchObject({ label: "R&R", days: -1 });
  });
});

describe("repository", () => {
  const now = new Date("2026-10-01T12:00:00Z");
  const repository = (
    patch: Partial<RepositoryStatus> = {},
  ): RepositoryStatus => ({
    branch: "main",
    head: "1111111",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    fetchedAt: "2026-10-01T11:00:00Z",
    changed: 0,
    untracked: 0,
    conflicts: 0,
    changedPaths: [],
    remote: {
      name: "origin",
      host: "github.com",
      owner: "mdroste",
      repo: "minwage",
      webUrl: "https://github.com/mdroste/minwage",
    },
    commits: [
      {
        sha: "1111111",
        author: "Ada Author",
        date: "2026-09-30T09:00:00Z",
        subject: "Tighten the introduction",
      },
    ],
    incoming: [],
    ...patch,
  });

  it("is absent when the folder is not a repository", () => {
    expect(repositoryBlock(NO_SIGNALS, now)).toBeNull();
  });

  it("says only where the repository lives when everything is committed and current", () => {
    const block = repositoryBlock(signals({ repository: repository() }), now);
    expect(block?.rows.map((item) => item.id)).toEqual(["identity"]);
    expect(row(block, "identity")).toMatchObject({
      text: "mdroste/minwage",
      action: { kind: "link", url: "https://github.com/mdroste/minwage" },
    });
  });

  it("reports uncommitted work, coauthor commits, and unpushed commits", () => {
    const block = repositoryBlock(
      signals({
        repository: repository({
          changed: 2,
          untracked: 1,
          changedPaths: ["paper/main.tex", "analysis/tables.do"],
          ahead: 1,
          behind: 2,
          upstream: "origin/feature/robustness",
          incoming: [
            {
              sha: "2",
              author: "Sam Lee",
              date: "2026-09-30T10:00:00Z",
              subject: "Add commuting-zone clusters",
            },
            {
              sha: "3",
              author: "Jane Doe",
              date: "2026-09-30T08:00:00Z",
              subject: "Fix Table 3",
            },
          ],
        }),
      }),
      now,
    );
    expect(row(block, "uncommitted")).toMatchObject({
      text: "2 uncommitted changes, 1 untracked file",
      detail: "paper/main.tex, analysis/tables.do",
    });
    expect(row(block, "incoming")).toMatchObject({
      text: "2 new commits on GitHub by Sam Lee and Jane Doe",
      attention: true,
      action: {
        kind: "link",
        url: "https://github.com/mdroste/minwage/compare/1111111...feature/robustness",
      },
    });
    expect(row(block, "remote")).toMatchObject({
      text: "1 commit not pushed to GitHub",
      action: { kind: "fetch", label: "Check GitHub" },
    });
  });

  it("offers a check when the remote has not been looked at for a day", () => {
    const stale = repositoryBlock(
      signals({
        repository: repository({ fetchedAt: "2026-09-28T11:00:00Z" }),
      }),
      now,
    );
    expect(row(stale, "remote")?.action).toEqual({
      kind: "fetch",
      label: "Check GitHub",
    });
    const never = repositoryBlock(
      signals({ repository: repository({ fetchedAt: null }) }),
      now,
    );
    expect(row(never, "remote")?.text).toBe(
      "GitHub has not been checked for new commits",
    );
  });

  it("names other hosts without inventing links to them", () => {
    const block = repositoryBlock(
      signals({
        repository: repository({
          behind: 1,
          remote: {
            name: "origin",
            host: "git.overleaf.com",
            owner: "",
            repo: "64f0c0ffee",
            webUrl: null,
          },
        }),
      }),
      now,
    );
    expect(row(block, "identity")?.action.kind).toBe("tab");
    expect(row(block, "incoming")).toMatchObject({
      text: "1 new commit on git.overleaf.com",
      action: { kind: "fetch" },
    });
  });

  it("lists commits made since the last visit", () => {
    const block = sinceBlock(home(), signals({ repository: repository() }), {
      since: "2026-09-29T00:00:00Z",
      baseline: null,
    });
    expect(row(block, "commits")).toMatchObject({
      text: "1 commit by Ada Author",
      detail: "Tighten the introduction",
      action: {
        kind: "link",
        url: "https://github.com/mdroste/minwage/commits/main",
      },
    });
    expect(
      sinceBlock(home(), signals({ repository: repository() }), {
        since: "2026-09-30T12:00:00Z",
        baseline: null,
      }),
    ).toBeNull();
  });
});
