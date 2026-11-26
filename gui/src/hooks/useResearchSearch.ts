import { useEffect, useMemo, useState } from "react";
import { projectClient } from "../lib/projectClient";
import { reviewHistoryClient } from "../lib/reviewHistoryClient";
import { detectReportIssues } from "../lib/issues";
import { listProjectReviewLinks } from "../lib/workbench/releaseClient";
import {
  reviewsForProject,
  type ProjectReviewLink,
} from "../lib/project-overview/projectPaths";
import {
  findingSearchEntries,
  projectSearchEntries,
  type ResearchSearchEntry,
  type DocumentSearchSource,
} from "../lib/researchSearch";
import type { Workspace } from "../lib/workbench/sessionTypes";
import type { RunSummary } from "../lib/reviewTypes";

/** Bounded, read-only discovery. Keystrokes filter the loaded records locally. */
export default function useResearchSearch(
  projects: Workspace[],
  runs: RunSummary[],
  projectId: string | null,
) {
  const [attempt, setAttempt] = useState(0);
  const [limit, setLimit] = useState(50);
  const [content, setContent] = useState<{
    entries: ResearchSearchEntry[];
    notices: string[];
    loading: boolean;
    links: Record<string, ProjectReviewLink[]>;
    documents: DocumentSearchSource[];
  }>({ entries: [], notices: [], loading: true, links: {}, documents: [] });
  const [findings, setFindings] = useState<{
    entries: ResearchSearchEntry[];
    notices: string[];
    loading: boolean;
  }>({ entries: [], notices: [], loading: true });
  const scopedProjects = useMemo(
    () => projects.filter((p) => !projectId || p.id === projectId),
    [projects, projectId],
  );
  useEffect(() => {
    let live = true;
    const pending = [...scopedProjects];
    const next: typeof content = {
      entries: [],
      notices: [],
      loading: true,
      links: {},
      documents: [],
    };
    setContent({
      entries: [],
      notices: [],
      loading: true,
      links: {},
      documents: [],
    });
    const worker = async () => {
      while (live && pending.length) {
        const project = pending.shift()!;
        const [home, links] = await Promise.allSettled([
          projectClient.home(project.id),
          listProjectReviewLinks(project.id),
        ]);
        if (!live) return;
        if (home.status === "fulfilled") {
          next.entries.push(...projectSearchEntries(project, home.value));
          for (const { paper, revision } of home.value.papers) {
            if (!paper.archived && revision)
              next.documents.push({ project, title: paper.title, revision });
          }
          const inventory = home.value.inventory?.body;
          for (const warning of inventory?.warnings ?? [])
            next.notices.push(`${project.name}: ${warning}`);
          const unsaved = home.value.papers.filter(
            ({ paper, revision }) => !paper.archived && !revision,
          ).length;
          if (unsaved)
            next.notices.push(
              `${project.name}: ${unsaved} documents have no saved version to search.`,
            );
          if (project.root && !inventory)
            next.notices.push(
              `${project.name}: file names unavailable. Refresh Files in the project.`,
            );
          else if (inventory && !inventory.complete)
            next.notices.push(
              `${project.name}: saved file list is incomplete.`,
            );
          if (home.value.notes.length >= 500 || home.value.papers.length >= 500)
            next.notices.push(
              `${project.name}: only the first 500 records of each type are available.`,
            );
        } else
          next.notices.push(
            `${project.name}: documents, notes, and file names unavailable.`,
          );
        if (links.status === "fulfilled") next.links[project.id] = links.value;
        else
          next.notices.push(
            `${project.name}: review links unavailable; project review matches may be incomplete.`,
          );
        setContent({
          ...next,
          entries: [...next.entries],
          notices: [...next.notices],
          links: { ...next.links },
          documents: [...next.documents],
        });
      }
    };
    void Promise.all([worker(), worker(), worker()]).then(() => {
      if (live)
        setContent({
          ...next,
          entries: [...next.entries],
          notices: [...next.notices],
          links: { ...next.links },
          documents: [...next.documents],
          loading: false,
        });
    });
    return () => {
      live = false;
    };
  }, [scopedProjects, attempt]);
  const runProjects = useMemo(() => {
    const map = new Map<string, Workspace>();
    for (const project of scopedProjects)
      for (const run of reviewsForProject(
        runs,
        project.root,
        content.links[project.id] ?? [],
      ))
        if (!map.has(run.run_id)) map.set(run.run_id, project);
    return map;
  }, [scopedProjects, runs, content.links]);
  const scopedRuns = useMemo(
    () =>
      runs
        .filter((run) => !projectId || runProjects.has(run.run_id))
        .sort((a, b) => (b.created ?? "").localeCompare(a.created ?? "")),
    [runs, projectId, runProjects],
  );
  useEffect(() => {
    let live = true;
    setFindings({ entries: [], notices: [], loading: true });
    if (content.loading)
      return () => {
        live = false;
      };
    const pending = scopedRuns.slice(0, limit);
    const entries: ResearchSearchEntry[] = [];
    const notices: string[] = [];
    const worker = async () => {
      while (live && pending.length) {
        const run = pending.shift()!;
        try {
          const report = await reviewHistoryClient.report({
            runId: run.run_id,
          });
          if (!live) return;
          entries.push(
            ...findingSearchEntries(
              run.run_id,
              run.title || run.input_name,
              detectReportIssues(report) ?? [],
              runProjects.get(run.run_id),
            ),
          );
        } catch {
          if (!live) return;
          notices.push(`Findings unavailable: ${run.title || run.input_name}.`);
        }
      }
    };
    void Promise.all([worker(), worker(), worker()]).then(() => {
      if (live) setFindings({ entries, notices, loading: false });
    });
    return () => {
      live = false;
    };
  }, [scopedRuns, runProjects, content.loading, attempt, limit]);
  return {
    entries: [...content.entries, ...findings.entries],
    notices: [...content.notices, ...findings.notices],
    loading: content.loading || findings.loading,
    runProjects,
    documents: content.documents,
    contentLoading: content.loading,
    scopedProjects,
    more: scopedRuns.length > limit,
    limit,
    loadMore: () => setLimit((n) => n + 50),
    retry: () => setAttempt((n) => n + 1),
  };
}
