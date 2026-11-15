import { listedRunStatus } from "../../lib/reportPresentation";
import type { Project } from "../../lib/reviewCollectionTypes";
import type { RunSummary } from "../../lib/reviewTypes";

export function selectHistoryRuns({
  runs,
  needle,
  dateFilter,
  profileFilter,
  projectFilter,
  projects,
  providerFilter,
  sortOrder,
  statusFilter,
}: {
  runs: RunSummary[];
  needle: string;
  dateFilter: string;
  profileFilter: string;
  projectFilter: string;
  projects: Project[];
  providerFilter: string;
  sortOrder: string;
  statusFilter: string;
}) {
  const matching = runs.filter((run) => {
    if (statusFilter !== "all" && listedRunStatus(run.status) !== statusFilter)
      return false;
    if (providerFilter !== "all" && run.provider !== providerFilter)
      return false;
    if (
      profileFilter !== "all" &&
      (run.profile_name || run.profile_id) !== profileFilter
    )
      return false;
    if (projectFilter !== "all") {
      const project = projects.find(
        (candidate) => candidate.id === projectFilter,
      );
      if (!project?.run_ids.includes(run.run_id)) return false;
    }
    if (dateFilter !== "all") {
      const age = Date.now() - Date.parse(run.created);
      if (
        !Number.isFinite(age) ||
        age > Number(dateFilter) * 24 * 60 * 60 * 1000
      )
        return false;
    }
    return (
      !needle ||
      [run.title, run.input_name, run.profile_name, run.provider, ...run.tags]
        .join(" ")
        .toLowerCase()
        .includes(needle)
    );
  });
  return matching.sort((left, right) => {
    if (sortOrder === "oldest")
      return Date.parse(left.created) - Date.parse(right.created);
    if (sortOrder === "name")
      return (left.title || left.input_name).localeCompare(
        right.title || right.input_name,
      );
    return Date.parse(right.created) - Date.parse(left.created);
  });
}
