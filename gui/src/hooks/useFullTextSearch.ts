import { useEffect, useMemo, useState } from "react";
import { researchClient } from "../lib/workbench/researchClient";
import { workspaceFileAdapter } from "../lib/fileWorkspaceClient";
import type { Workspace } from "../lib/workbench/sessionTypes";
import type {
  DocumentSearchSource,
  ResearchSearchEntry,
} from "../lib/researchSearch";

// Native reads cannot be aborted. Keep the bound across query changes and
// dialog remounts, and discard obsolete queued reads before invoking native code.
let activeReads = 0;
const waiting: (() => void)[] = [];
async function readWhenAvailable(
  action: () => Promise<void>,
  current: () => boolean,
) {
  await new Promise<void>((resolve) => {
    const start = () => {
      activeReads++;
      resolve();
    };
    if (activeReads < 3) start();
    else waiting.push(start);
  });
  try {
    if (current()) await action();
  } finally {
    activeReads--;
    waiting.shift()?.();
  }
}

interface Results {
  key: string;
  entries: ResearchSearchEntry[];
  notices: string[];
  documents: number;
  files: number;
  more: boolean;
  loading: boolean;
}
const empty = (key: string, loading = false): Results => ({
  key,
  entries: [],
  notices: [],
  documents: 0,
  files: 0,
  more: false,
  loading,
});

/** Searches captured text and current project files locally, without extraction. */
export default function useFullTextSearch(
  projects: Workspace[],
  documents: DocumentSearchSource[],
  query: string,
  kind: string,
  sourcesLoading: boolean,
) {
  const phrase = query.trim();
  const enabled = ["All types", "Documents", "Files"].includes(kind);
  const valid =
    [...phrase].length >= 2 && new TextEncoder().encode(phrase).length <= 500;
  const [attempt, setAttempt] = useState(0);
  const [depth, setDepth] = useState({ query: "", limit: 20 });
  const limit = depth.query === phrase ? depth.limit : 20;
  const key = JSON.stringify([
    phrase,
    kind,
    projects.map((p) => p.id),
    attempt,
    limit,
  ]);
  const [result, setResult] = useState<Results>(() => empty(key));
  const sources = useMemo(
    () => documents.filter((d) => projects.some((p) => p.id === d.project.id)),
    [documents, projects],
  );
  useEffect(() => {
    let current = true;
    const next = empty(key, enabled && valid);
    setResult(empty(key, enabled && valid));
    if (!enabled || !valid || sourcesLoading)
      return () => {
        current = false;
      };
    const publish = () => {
      if (current)
        setResult({
          ...next,
          entries: [...next.entries],
          notices: [...next.notices],
        });
    };
    const timer = setTimeout(() => {
      const jobs: (() => Promise<void>)[] = [];
      if (kind !== "Files")
        for (const source of sources) {
          const { project, revision, title } = source;
          if (!revision.textReference) {
            next.notices.push(
              `${project.name} · ${title}: no captured text. Import a text version to search its contents.`,
            );
            continue;
          }
          jobs.push(async () => {
            try {
              const hits = await researchClient.paperSearch(
                project.id,
                revision.id,
                phrase,
                limit + 1,
              );
              if (!current) return;
              next.documents++;
              if (!revision.captureComplete)
                next.notices.push(
                  `${project.name} · ${title}: captured text is incomplete.`,
                );
              if (hits.length > limit) {
                next.more ||= limit < 60;
                next.notices.push(
                  `${project.name} · ${title}: showing the first ${limit} passages${limit < 60 ? "; load more below" : "; narrow your search for later matches"}.`,
                );
              }
              for (const hit of hits.slice(0, limit))
                next.entries.push({
                  id: `passage:${project.id}:${hit.revisionId}:${hit.start}`,
                  group: "Documents",
                  label: title,
                  text: hit.excerpt,
                  matchedQuery: phrase.toLocaleLowerCase(),
                  hint: `${project.name} · Version ${hit.contentHash.slice(0, 8)} · ${hit.page ? `Page ${hit.page}` : `Line ${hit.line}`}`,
                  route: {
                    page: "workspace",
                    projectId: project.id,
                    surface: "project",
                    destination: "documents",
                    revisionId: hit.revisionId,
                    documentStart: hit.start,
                    documentEnd: hit.end,
                    documentHash: hit.contentHash,
                    documentPage: hit.page ?? undefined,
                  },
                });
            } catch {
              if (current)
                next.notices.push(
                  `${project.name} · ${title}: document text search unavailable. Retry to search this version.`,
                );
            }
            publish();
          });
        }
      if (kind !== "Documents")
        for (const project of projects.filter((p) => p.root))
          jobs.push(async () => {
            try {
              const found = await workspaceFileAdapter({
                workspaceId: project.id,
              }).search!(phrase);
              if (!current) return;
              next.files += found.searchedFiles;
              if (found.skippedFiles)
                next.notices.push(
                  `${project.name}: ${found.skippedFiles} file entries could not be searched (size, encoding, or access).`,
                );
              if (found.truncated)
                next.notices.push(
                  `${project.name}: file search reached its time or result limit. Narrow the phrase or search inside Files.`,
                );
              for (const hit of found.matches)
                next.entries.push({
                  id: `file-text:${project.id}:${hit.path}:${hit.line}`,
                  group: "Files",
                  label: hit.path.split(/[\\/]/).pop() || hit.path,
                  hint: `${project.name} · Current file · ${hit.path}:${hit.line}`,
                  text: hit.text,
                  matchedQuery: phrase.toLocaleLowerCase(),
                  route: {
                    page: "workspace",
                    projectId: project.id,
                    surface: "project",
                    destination: "files",
                    file: hit.path,
                    fileLine: hit.line,
                  },
                });
            } catch {
              if (current)
                next.notices.push(
                  `${project.name}: file text search unavailable. Check that the project folder is accessible, then retry.`,
                );
            }
            publish();
          });
      publish();
      void Promise.all(
        jobs.map((job) => readWhenAvailable(job, () => current)),
      ).then(() => {
        next.loading = false;
        publish();
      });
    }, 300);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [
    key,
    enabled,
    valid,
    sourcesLoading,
    sources,
    projects,
    phrase,
    kind,
    limit,
  ]);
  const visible = result.key === key ? result : empty(key, enabled && valid);
  return {
    ...visible,
    enabled,
    valid,
    retry: () => setAttempt((n) => n + 1),
    loadMore: () =>
      setDepth({ query: phrase, limit: Math.min(limit + 20, 60) }),
  };
}
