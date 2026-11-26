import { reference, type ContextItem } from "./deskClient";
import { studioClient, type SourceContextRequest } from "./studioClient";

export async function captureSourceContext(
  workspaceId: string,
  source: SourceContextRequest,
): Promise<ContextItem[]> {
  const snapshot = await studioClient.mutate(workspaceId, {
    action: "captureContext",
    source,
  });
  return [{ role: "source", object: reference(snapshot) }];
}

export const researchDraftInstruction =
  "Separate observed evidence from interpretation. Suggest an inspectable edit, note, or response draft. State which checks actually ran and their limits; do not claim a model assertion was verified.";
