import { workflowProfileClient } from "./workflowProfileClient";

/** Where a workflow's unsaved edits wait out a crash. A copy lasts until the
 *  edits are saved or the researcher chooses to throw them away. */
export const workflowDraftKey = (profile: string) =>
  `pipeline.workflowDraft.${profile}`;

export function forgetWorkflowDraft(profile: string) {
  try {
    window.localStorage.removeItem(workflowDraftKey(profile));
  } catch {
    // Unavailable storage holds nothing that could be offered back.
  }
}

/** For a discard confirmed outside the designer, which does not know which
 *  workflow is open. If that can't be read, the next visit asks again. */
export async function forgetActiveWorkflowDraft() {
  try {
    forgetWorkflowDraft(await workflowProfileClient.active());
  } catch {
    // Keeping the copy loses nothing.
  }
}
