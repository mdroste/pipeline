import {
  legacyManuscriptPaths,
  legacyManuscriptDraft,
} from "../../lib/manuscriptDraftStorage";
// A build includes drafts retained when switching files, in this working copy
// only. Each save uses its original hash; conflicts stop before compilation.
import { studioClient } from "../../lib/studioClient";
import {
  projectFileScope,
  readFileDraft,
  writeFileDraft,
  listFileDraftPaths,
} from "../../lib/sharedFileDrafts";

export async function saveRetainedManuscriptDrafts(
  workspaceId: string,
  checkpoint: string,
  currentPath?: string,
) {
  const scope = projectFileScope(workspaceId, checkpoint);
  const sharedPaths = listFileDraftPaths(scope);
  // Resolve old duplicate buffers before writing any shared file.
  for (const path of sharedPaths) {
    const retained = legacyManuscriptDraft(workspaceId, checkpoint, path);
    const legacy = retained.value;
    if (legacy && legacy.draft !== readFileDraft(scope, path)?.draft)
      throw new Error(
        "An earlier manuscript draft differs from the shared draft. Open that file in Write and resolve the recovered draft before building.",
      );
    if (legacy) retained.remove();
  }
  for (const path of sharedPaths) {
    if (path === currentPath || !/\.(tex|bib|sty|cls|bst)$/i.test(path))
      continue;
    const value = readFileDraft(scope, path);
    if (!value)
      throw new Error(
        "A retained draft could not be read. Open that file before building.",
      );
    const result = await studioClient.mutate(workspaceId, {
      action: "saveText",
      checkpointId: checkpoint || null,
      path,
      expectedHash: value.file.hash,
      content: value.draft,
    });
    const latest = readFileDraft(scope, path);
    writeFileDraft(scope, {
      file: { ...value.file, text: value.draft, hash: result.hash },
      draft: latest?.draft ?? value.draft,
    });
    if (latest && latest.draft !== value.draft)
      throw new Error(
        "A file was edited while preparing the build. Review and build again.",
      );
  }
  for (const path of legacyManuscriptPaths(workspaceId, checkpoint).sort()) {
    const retained = legacyManuscriptDraft(workspaceId, checkpoint, path);
    if (!retained.raw || !retained.value) continue;
    const value = retained.value;
    if (value.file?.path === currentPath) continue;
    if (readFileDraft(scope, value.file?.path))
      throw new Error(
        "An earlier manuscript draft differs from the shared draft. Open that file in Write and resolve the recovered draft before building.",
      );
    if (
      typeof value.draft !== "string" ||
      typeof value.file?.hash !== "string" ||
      value.file.checkpointId !== (checkpoint || null) ||
      path !== value.file.path
    )
      throw new Error(
        "A retained draft could not be read. Open that file before building.",
      );
    if (value.draft !== value.file.content) {
      const result = await studioClient.mutate(workspaceId, {
        action: "saveText",
        checkpointId: value.file.checkpointId,
        path: value.file.path,
        expectedHash: value.file.hash,
        content: value.draft,
      });
      writeFileDraft(scope, {
        file: { path: value.file.path, hash: result.hash, text: value.draft },
        draft: value.draft,
      });
    }
    retained.removeUnchanged();
  }
}
