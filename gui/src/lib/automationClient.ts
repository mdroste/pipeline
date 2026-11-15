/** Shared list and selection records; detail reads and controls keep their owning clients. */
import { invoke } from "@tauri-apps/api/core";
import { taskClient } from "./taskClient";
import { missionClient, missionState, type Mission } from "./missionClient";
import {
  discoveryClient,
  discoveryStatus,
  type DiscoveryRun,
} from "./discoveryClient";
import type { TaskRun } from "./taskTypes";

export type AutomationKind = "task" | "research" | "discovery";
export type AutomationRef = { kind: AutomationKind; id: string };
export interface AutomationSummary extends AutomationRef {
  name: string;
  state: string;
  reason: string | null;
  updatedAt: number;
  dueAt: number | null;
}
export type AutomationDetail =
  | { kind: "task"; run: TaskRun }
  | { kind: "research"; run: Mission }
  | { kind: "discovery"; run: DiscoveryRun };

export const automationKindLabel = (kind: AutomationKind) =>
  ({
    task: "Steps",
    research: "Investigation",
    discovery: "Discovery",
  })[kind];
export const researchAutomationStatus = (
  kind: "research" | "discovery",
  state: string,
) => (kind === "research" ? missionState(state) : discoveryStatus(state));

export const automationClient = {
  list: (view: string, offset = 0) =>
    invoke<AutomationSummary[]>("automation_list", { view, offset }),
  get: async (ref: AutomationRef): Promise<AutomationDetail> => {
    if (ref.kind === "research")
      return { kind: "research", run: await missionClient.get(ref.id) };
    if (ref.kind === "discovery")
      return { kind: "discovery", run: await discoveryClient.get(ref.id) };
    return { kind: "task", run: await taskClient.get(ref.id) };
  },
};
