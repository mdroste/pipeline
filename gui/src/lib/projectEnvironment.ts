import { createContext, useContext } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { workspaceClient } from "./workbench/workspaceClient";
import { researchClient } from "./workbench/researchClient";
import { projectClient } from "./projectClient";
import { harnessClient } from "./workbench/harnessClient";
import { router } from "./router";
import { readActivitySince as recordVisit } from "./project-overview/visits";
import type { ProjectSignals } from "./project-overview/types";

/** Scoped services let the real project controls also run against a disposable sample. */
export interface ProjectEnvironmentValue {
  workspaceClient: Pick<
    typeof workspaceClient,
    "createWorkspace" | "registerWorkspaceRoot"
  >;
  researchClient: Pick<
    typeof researchClient,
    "importPaper" | "createNote" | "updateNote"
  >;
  projectClient: Pick<typeof projectClient, "home" | "mutate" | "taskPage">;
  harnessClient: Pick<
    typeof harnessClient,
    "getWorkspaceConfig" | "saveWorkspaceConfig"
  >;
  choose: typeof open;
  navigate: typeof router.navigate;
  openExternal: typeof openExternal;
  recordVisit: typeof recordVisit;
  signals: ProjectSignals | null;
  /** The surrounding tutorial owns focus and background isolation. */
  embeddedDialogs: boolean;
}
export const defaultProjectEnvironment: ProjectEnvironmentValue = {
  workspaceClient,
  researchClient,
  projectClient,
  harnessClient,
  choose: open,
  // Router methods read `this`; consumers destructure this field and call it bare.
  navigate: (route) => router.navigate(route),
  openExternal,
  recordVisit,
  signals: null,
  embeddedDialogs: false,
};
export const ProjectEnvironment = createContext<ProjectEnvironmentValue>(
  defaultProjectEnvironment,
);
export const useProjectEnvironment = () => useContext(ProjectEnvironment);
