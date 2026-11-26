import { invoke } from "@tauri-apps/api/core";
import type {
  DeskRecord,
  OpenResearchObject,
  ResearchObject,
  ContextSelection,
  ContextItem,
} from "./deskTypes";
export type * from "./deskTypes";
export const operation = () => `desk-${crypto.randomUUID()}`;
export const reference = (r: DeskRecord<unknown>): OpenResearchObject => ({
  kind: r.kind,
  id: r.id,
  revision: r.contentHash,
});
export const deskClient = {
  records: <T = Record<string, unknown>>(workspaceId: string, kind: string) =>
    invoke<DeskRecord<T>[]>("workbench_desk_records", { workspaceId, kind }),
  read: (workspaceId: string, object: OpenResearchObject) =>
    invoke<ResearchObject>("workbench_research_object", {
      workspaceId,
      object,
    }),
  context: (sessionId: string) =>
    invoke<ContextSelection>("workbench_context_selection", { sessionId }),
  saveContext: (
    sessionId: string,
    selection: ContextSelection,
    items: ContextItem[],
  ) =>
    invoke<ContextSelection>("workbench_save_context_selection", {
      sessionId,
      expectedRevision: selection.revision,
      items,
    }),
};
