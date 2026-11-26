import { workspaceFileAdapter } from "./fileWorkspaceClient";
/** Inspect names and TeX headers only. Opening research never runs its code. */
export async function findProjectRoots(workspaceId: string): Promise<string[]> {
  const adapter = workspaceFileAdapter({ workspaceId });
  const root = await adapter.list!("");
  const directories = root.entries
    .filter(
      (entry) =>
        entry.kind === "folder" &&
        !entry.ignored &&
        !entry.name.startsWith("."),
    )
    .slice(0, 12);
  const pages = await Promise.allSettled(
    directories.map((entry) => adapter.list!(entry.path)),
  );
  const files = [
    root,
    ...pages.flatMap((page) =>
      page.status === "fulfilled" ? [page.value] : [],
    ),
  ]
    .flatMap((page) => page.entries)
    .filter((entry) => entry.kind === "file" && !entry.ignored);
  const tex = files.filter((file) => /\.tex$/i.test(file.path)).slice(0, 30);
  const checked = await Promise.allSettled(
    tex.map(async (file) => {
      const preview = await adapter.read(file.path);
      try {
        return /\\documentclass\s*(?:\[[^\]]*\])?\s*\{/.test(
          (preview.text ?? "").replace(/%[^\n]*/g, ""),
        )
          ? file.path
          : null;
      } finally {
        if (preview.streamToken) adapter.release?.(preview.streamToken);
      }
    }),
  );
  const roots = checked.flatMap((result) =>
    result.status === "fulfilled" && result.value ? [result.value] : [],
  );
  if (roots.length) return roots.sort();
  return files
    .filter(
      (file) =>
        /\.(pdf|docx|md|qmd)$/i.test(file.path) &&
        !/(^|\/)(readme|changelog|license|agents)\./i.test(file.path),
    )
    .map((file) => file.path)
    .sort()
    .slice(0, 30);
}
