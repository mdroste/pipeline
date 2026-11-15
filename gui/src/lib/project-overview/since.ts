import { authors, refPath } from "./repository";
import type { OverviewBlock, OverviewRow, ProjectSignals } from "./types";
import type { ProjectHome } from "../projectClient";
import type { Visit } from "./visits";
import { after, block, clip, overviewDay, plural, time } from "./presentation";
import { diffFiles, snapshotFiles } from "./visits";
import { listedRunStatus, runFinished } from "../reportPresentation";

/** Since you were last here: what changed without the researcher. */
export function sinceBlock(
  data: ProjectHome,
  signals: ProjectSignals,
  visit: Visit,
): OverviewBlock | null {
  if (!visit.since) return null;
  const since = Date.parse(visit.since);
  if (!Number.isFinite(since)) return null;
  const rows: OverviewRow[] = [];

  const current = snapshotFiles(data.inventory?.body.files);
  if (visit.baseline && current) {
    const diff = diffFiles(visit.baseline, current);
    const paths = [...diff.changed, ...diff.added, ...diff.removed];
    if (paths.length)
      rows.push({
        id: "files",
        text: `${plural(paths.length, "file")} changed in the project folder`,
        detail: [
          paths.slice(0, 3).join(", "),
          paths.length > 3 ? `and ${paths.length - 3} more` : "",
        ]
          .filter(Boolean)
          .join(" "),
        action: { kind: "tab", label: "Open files", tab: "files" },
      });
  }

  for (const paper of data.papers)
    if (paper.revision && after(paper.revision.capturedAt, since))
      rows.push({
        id: `version-${paper.revision.id}`,
        text: `New version captured: ${paper.paper.title}`,
        detail: overviewDay(paper.revision.capturedAt),
        action: {
          kind: "document",
          label: "Open",
          revisionId: paper.revision.id,
        },
      });

  const repository = signals.repository;
  const commits = (repository?.commits ?? []).filter((commit) =>
    after(commit.date, since),
  );
  if (commits.length) {
    const web = repository?.remote?.webUrl;
    rows.push({
      id: "commits",
      text: `${plural(commits.length, "commit")} by ${authors(commits)}`,
      detail: clip(
        commits
          .slice(0, 2)
          .map((commit) => commit.subject)
          .join("; "),
      ),
      action:
        web && repository?.branch
          ? {
              kind: "link",
              label: "View on GitHub",
              url: `${web}/commits/${refPath(repository.branch)}`,
            }
          : { kind: "tab", label: "Open files", tab: "files" },
    });
  }

  const reviews = (signals.runs ?? [])
    .filter(
      (run) =>
        run.status !== "running" &&
        time(run.created) + run.duration_secs * 1000 > since,
    )
    .sort((a, b) => time(b.created) - time(a.created));
  for (const run of reviews.slice(0, 3))
    rows.push({
      id: `review-${run.run_id}`,
      text: `Review finished: ${run.title || run.input_name}`,
      detail: [run.profile_name, listedRunStatus(run.status)]
        .filter(Boolean)
        .join(" · "),
      attention: !runFinished(run.status),
      action: { kind: "run", label: "Open report", runId: run.run_id },
    });

  const automations = (signals.automations ?? []).filter(
    (task) =>
      ["finished", "failed", "cancelled", "attention"].includes(task.state) &&
      after(task.updatedAt, since),
  );
  for (const task of automations.slice(0, 3))
    rows.push({
      id: `automation-${task.id}`,
      text: `Automation ${
        task.state === "attention"
          ? "needs attention"
          : task.state === "finished"
            ? "finished"
            : task.state === "failed"
              ? "failed"
              : "stopped"
      }: ${task.name}`,
      detail: task.reason ? clip(task.reason) : undefined,
      attention: ["failed", "attention"].includes(task.state),
      action: { kind: "automation", label: "Open", taskId: task.id },
    });

  return block("since", "Since you were last here", rows);
}
