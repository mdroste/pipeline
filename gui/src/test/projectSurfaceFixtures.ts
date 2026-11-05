import type { ProjectHome } from "../lib/projectClient";

export function makeProjectHome(): ProjectHome {
  return {
    settings: {
      id: "home",
      workspaceId: "workspace",
      kind: "home",
      revision: 4,
      updatedAt: "now",
      body: {
        manuscriptRevisionId: null,
        baselineExecutionId: null,
        briefNoteIds: [],
        excludedNoteIds: [],
        ignoredPaths: [],
        layout: "reading",
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
    inventory: {
      id: "inventory",
      workspaceId: "workspace",
      kind: "inventory",
      revision: 1,
      updatedAt: "now",
      body: {
        rootIdentity: "root",
        capturedAt: "now",
        complete: true,
        warnings: [],
        files: [
          {
            path: "paper.tex",
            hash: "hash",
            size: 20,
            executable: false,
            status: "current",
          },
        ],
      },
    },
    changes: [],
    applications: [],
    workingCopyStatus: "Refresh files",
    fileAcceptance: true,
    contextPreview: "Reference results run: not selected.",
  };
}
