import type { RunSummary } from "../reviewTypes";
/**
 * One spelling per native path. A Windows project root is canonical
 * (`\\?\C:\…`, `\\?\UNC\server\share\…`), while a review's input is the path
 * the file dialog returned (`C:\…`), in whatever case the user saw. Windows
 * paths drop the verbatim prefix, use `/`, and compare case-insensitively.
 */
function comparablePath(value: string): string {
  let path = value;
  if (/^\\\\\?\\UNC\\/i.test(path)) path = `\\\\${path.slice(8)}`;
  else if (/^\\\\\?\\[a-z]:/i.test(path)) path = path.slice(4);
  const windows = /^[a-z]:([\\/]|$)/i.test(path) || /^\\\\[^\\]/.test(path);
  if (!windows) return path.replace(/\/+$/, "");
  return path.replaceAll("\\", "/").replace(/\/+$/, "").toLowerCase();
}

export function underRoot(path: string, root: string) {
  const base = comparablePath(root);
  const candidate = comparablePath(path);
  return candidate === base || candidate.startsWith(`${base}/`);
}

export interface ProjectReviewLink {
  id: string;
  stagedPath: string;
  externalReference: string | null;
  contentHash: string;
  paperId?: string;
  revisionId?: string;
}

export function reviewsForProject(
  runs: RunSummary[],
  root: string | null,
  links: ProjectReviewLink[],
) {
  const byId = new Map(links.map((link) => [link.id, link]));
  const paths = new Set(
    links.flatMap((link) =>
      link.stagedPath ? [comparablePath(link.stagedPath)] : [],
    ),
  );
  const ids = new Set(
    links.flatMap((link) =>
      link.externalReference ? [link.externalReference] : [],
    ),
  );
  return runs.filter((run) => {
    if (
      ids.has(run.run_id) ||
      (run.input_identity?.review_handoff_id &&
        byId.has(run.input_identity.review_handoff_id))
    )
      return true;
    const path = comparablePath(run.input_path);
    if (!path) return false;
    if (paths.has(path) || (root && underRoot(run.input_path, root)))
      return true;
    // Before the run stored a handoff id, the immutable staging layout named
    // both the handoff and the content-addressed blob. Its root can relocate.
    const [name, id, folder] = path.split("/").reverse();
    const legacy = folder === "handoffs" ? byId.get(id) : undefined;
    if (!legacy?.contentHash) return false;
    return (
      name === "source-tree-" + legacy.contentHash ||
      name.split(".")[0] === legacy.contentHash
    );
  });
}

/** Only an explicit handoff identifies a document; a shared project folder does not. */
export function reviewDocumentId(
  run: RunSummary,
  links: ProjectReviewLink[],
): string | null {
  return (
    links.find(
      (link) =>
        link.id === run.input_identity?.review_handoff_id ||
        link.externalReference === run.run_id ||
        (link.stagedPath &&
          comparablePath(link.stagedPath) === comparablePath(run.input_path)),
    )?.paperId ?? null
  );
}
