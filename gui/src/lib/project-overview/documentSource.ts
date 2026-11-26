import type { PaperWithRevision } from "../workbench/researchTypes";
import { resolveFileLink } from "../fileLinks";

/** A source belongs to the attached folder. Ambiguous TeX roots stay unchosen. */
export function documentSourcePath(
  document: PaperWithRevision,
  root: string | null | undefined,
  paths: string[],
) {
  const entry = document.revision?.entrypoint?.replace(/[\\/]+$/, "");
  const folder = root?.replace(/[\\/]+$/, "");
  if (!entry || !folder) return null;
  const windows = /^[a-z]:|^\\\\/i.test(folder);
  const comparable = (path: string) => (windows ? path.toLowerCase() : path);
  if (document.revision?.inputKind !== "source_tree") {
    if (!/\.(tex|md|txt|bib)$/i.test(entry)) return null;
    const path = resolveFileLink("", encodeURIComponent(entry), folder)?.path;
    return path
      ? (paths.find(
          (candidate) => comparable(candidate) === comparable(path),
        ) ?? path)
      : null;
  }
  const prefix = resolveFileLink(
    "",
    encodeURIComponent(`${entry}${entry.includes("\\") ? "\\" : "/"}_source_`),
    folder,
  )?.path.replace(/_source_$/, "");
  if (prefix === undefined || prefix === null) return null;
  const candidates = paths.filter(
    (path) =>
      comparable(path).startsWith(comparable(prefix)) && /\.tex$/i.test(path),
  );
  return (
    candidates.find(
      (path) => comparable(path) === comparable(`${prefix}main.tex`),
    ) ?? (candidates.length === 1 ? candidates[0] : null)
  );
}
