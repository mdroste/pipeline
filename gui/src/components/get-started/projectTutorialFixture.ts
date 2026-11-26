import type {
  ProjectAction,
  ProjectHome,
  ProjectRecord,
} from "../../lib/projectClient";
import type {
  Workspace,
  WorkbenchSession,
} from "../../lib/workbench/sessionTypes";
import type { ProjectSignals } from "../../lib/project-overview/types";
import type { ProjectEnvironmentValue } from "../../lib/projectEnvironment";
import { EXAMPLE_PAPER } from "./exampleReviewPaper";
import { EXAMPLE_SUMMARY } from "./exampleReviewData";
import { EXAMPLE_FINDINGS } from "./exampleReviewFindings";
import { SAMPLE_PATH } from "./reviewTutorialFixture";

const ID = "tutorial-port-harlan";
const AT = "2026-03-16T12:00:00Z";
export const PROJECT_QUESTION =
  "Does the Blue Line increase women's employment, and do the identification assumptions support a causal interpretation?";
export const PROJECT_TASK =
  "Check the event-study pre-trends and comparison group";
const record = <T>(id: string, kind: string, body: T): ProjectRecord<T> => ({
  id,
  kind,
  body,
  workspaceId: ID,
  revision: 1,
  updatedAt: AT,
});

export function createProjectTutorialData() {
  const workspace: Workspace = {
    id: ID,
    name: "Port Harlan transit study",
    root: "Example",
    rootIdentity: "tutorial",
    settingsRevision: 1,
    revision: 1,
    archivedAt: null,
    missingRootAt: null,
    createdAt: AT,
    updatedAt: AT,
  };
  const session: WorkbenchSession = {
    id: `${ID}-chat`,
    workspaceId: ID,
    paperId: null,
    title: "Identification and the event study",
    draft:
      "Help me assess whether the pre-trends support the parallel-trends assumption.",
    presetId: null,
    overrides: {},
    revision: 1,
    archivedAt: null,
    createdAt: AT,
    updatedAt: AT,
  };
  const home: ProjectHome = {
    settings: record("home", "home", {
      manuscriptRevisionId: `${ID}-revision`,
      baselineExecutionId: null,
      briefNoteIds: ["question"],
      excludedNoteIds: [],
      ignoredPaths: [],
      layout: "reading",
      kind: "empirical",
      stage: "drafting",
    }),
    papers: [
      {
        paper: {
          id: `${ID}-paper`,
          workspaceId: ID,
          title: EXAMPLE_PAPER.title,
          role: "manuscript",
          documentType: "paper",
          currentRevisionId: `${ID}-revision`,
          createdAt: AT,
          updatedAt: AT,
        },
        revision: {
          id: `${ID}-revision`,
          paperId: `${ID}-paper`,
          inputKind: "document",
          entrypoint: SAMPLE_PATH,
          dependencyManifest: {},
          contentHash: "port-harlan-example",
          textReference: null,
          compiledArtifactId: null,
          extraction: {},
          captureComplete: true,
          capturedAt: "2026-03-15T12:00:00Z",
        },
      },
    ],
    notes: [
      {
        id: "question",
        workspaceId: ID,
        paperId: null,
        kind: "question",
        body: PROJECT_QUESTION,
        state: "accepted",
        origin: "user",
        pinned: true,
        revision: 1,
        createdAt: AT,
        updatedAt: AT,
      },
      {
        id: "assumption",
        workspaceId: ID,
        paperId: null,
        kind: "assumption",
        body: "Station and comparison tracts would have followed parallel employment trends without the Blue Line.",
        state: "proposed",
        origin: "assistant",
        pinned: false,
        revision: 1,
        createdAt: AT,
        updatedAt: AT,
      },
    ],
    tasks: [
      record("pre-trends", "task", {
        objective: PROJECT_TASK,
        anchorId: null,
        expectedOutputs: ["A revised discussion of identification"],
        expectedChecks: ["Compare pre-treatment coefficients"],
        status: "open",
      }),
    ],
    noteHistory: [],
    anchors: [],
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
    contextPreview: PROJECT_QUESTION,
    workingCopyStatus: "Sample document is current",
    fileAcceptance: false,
  };
  return { workspace, session, home };
}

export const PROJECT_SIGNALS: ProjectSignals = {
  runs: [
    {
      ...EXAMPLE_SUMMARY,
      input_path: SAMPLE_PATH,
      input_identity: {
        content_hash: "port-harlan-example",
        selection_key: "example",
        lineage_id: "example",
      },
    },
  ],
  ledgers: [
    {
      schema_version: 1,
      project_id: ID,
      updated: AT,
      warnings: [],
      issues: EXAMPLE_FINDINGS.map((finding) => ({
        id: finding.id,
        title: finding.title,
        severity: finding.priority,
        section: finding.category,
        status: "open",
        note: "",
        created: AT,
        updated: AT,
        decision_updated: AT,
        occurrences: [],
      })),
    },
  ],
  builds: [],
  responses: [],
  automations: [],
  repository: null,
};

/** All sample writes stay in this instance. No native client is copied into it. */
export function createProjectTutorialEnvironment(
  sample: ReturnType<typeof createProjectTutorialData>,
  changed: () => void,
  navigate: ProjectEnvironmentValue["navigate"],
): ProjectEnvironmentValue {
  const mutate = async <T>(
    _workspace: string,
    action: ProjectAction,
  ): Promise<ProjectRecord<T>> => {
    let result: ProjectRecord<unknown>;
    if (action.action === "saveHome") {
      result = sample.home.settings = {
        ...sample.home.settings,
        body: action.settings,
        revision: sample.home.settings.revision + 1,
      };
    } else if (action.action === "updateTask") {
      const task = sample.home.tasks.find((item) => item.id === action.taskId);
      if (!task) throw new Error("Sample action item not found.");
      result = {
        ...task,
        revision: task.revision + 1,
        body: { ...task.body, status: action.status },
      };
      sample.home.tasks = sample.home.tasks.map((item) =>
        item.id === task.id ? (result as typeof task) : item,
      );
    } else if (action.action === "createTask") {
      const task = record(`task-${crypto.randomUUID()}`, "task", {
        objective: action.objective,
        anchorId: action.anchorId,
        expectedOutputs: action.expectedOutputs,
        expectedChecks: action.expectedChecks,
        status: "open",
      });
      sample.home.tasks = [task, ...sample.home.tasks];
      result = task;
    } else throw new Error("This operation is outside the sample project.");
    changed();
    return result as ProjectRecord<T>;
  };
  return {
    embeddedDialogs: true,
    signals: PROJECT_SIGNALS,
    recordVisit: () => ({ since: null, baseline: null }),
    choose: (async (options) =>
      options?.directory
        ? "Example"
        : SAMPLE_PATH) as ProjectEnvironmentValue["choose"],
    navigate,
    openExternal: async () => {},
    projectClient: {
      home: async () => sample.home,
      mutate,
      taskPage: async () => ({ records: [], nextCursor: null }),
    },
    workspaceClient: {
      createWorkspace: async ({ name, root }) => {
        sample.workspace = { ...sample.workspace, name, root };
        changed();
        return { record: sample.workspace, sequence: 1 };
      },
      registerWorkspaceRoot: async ({ root }) => {
        sample.workspace = { ...sample.workspace, root };
        changed();
        return { record: sample.workspace, sequence: 2 };
      },
    },
    researchClient: {
      importPaper: async () => sample.home.papers[0],
      createNote: async (request) => {
        const note = {
          ...request,
          id: `note-${crypto.randomUUID()}`,
          state: request.state ?? ("accepted" as const),
          revision: 1,
          createdAt: AT,
          updatedAt: AT,
        };
        sample.home.notes = [...sample.home.notes, note];
        changed();
        return note;
      },
      updateNote: async ({
        noteId,
        expectedRevision: _revision,
        operationId: _operation,
        ...patch
      }) => {
        const previous = sample.home.notes.find((note) => note.id === noteId);
        if (!previous) throw new Error("Sample note not found.");
        const updated = {
          ...previous,
          ...patch,
          revision: previous.revision + 1,
        };
        sample.home.notes = sample.home.notes.map((note) =>
          note.id === noteId ? updated : note,
        );
        if (patch.body)
          sample.home.noteHistory = [
            ...sample.home.noteHistory,
            { noteId, details: { previous }, createdAt: AT },
          ];
        changed();
        return updated;
      },
    },
  };
}
