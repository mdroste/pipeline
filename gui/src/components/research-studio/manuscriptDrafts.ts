// A build includes drafts retained when switching files, in this working copy
// only. Each save uses its original hash; conflicts stop before compilation.
import { studioClient, type EditorFile } from "../../lib/studioClient";

export async function saveRetainedManuscriptDrafts(
  workspaceId: string,
  checkpoint: string,
  currentPath?: string,
) {
  const base = `pipeline.manuscriptDraft.${workspaceId}`;
  const prefix = `${base}.${checkpoint || "working"}.`;
  const keys = Array.from({ length: localStorage.length }, (_, i) =>
    localStorage.key(i),
  )
    .filter((key): key is string => key !== null && key.startsWith(prefix))
    .sort();
  for (const key of keys) {
    const raw = localStorage.getItem(key);
    if (!raw) continue;
    const value = JSON.parse(raw) as { file: EditorFile; draft: string };
    if (value.file?.path === currentPath) continue;
    if (
      typeof value.draft !== "string" ||
      typeof value.file?.hash !== "string" ||
      value.file.checkpointId !== (checkpoint || null) ||
      key !== prefix + value.file.path
    )
      throw new Error(
        "A retained draft could not be read. Open that file before building.",
      );
    if (value.draft !== value.file.content)
      await studioClient.mutate(workspaceId, {
        action: "saveText",
        checkpointId: value.file.checkpointId,
        path: value.file.path,
        expectedHash: value.file.hash,
        content: value.draft,
      });
    if (localStorage.getItem(key) === raw) localStorage.removeItem(key);
    if (localStorage.getItem(base) === raw) localStorage.removeItem(base);
  }
}
