import type { OverviewBlock, OverviewRow, ProjectSignals } from "./types";
import { block, clip, overviewDay, plural, time } from "./presentation";
import { remoteLabel } from "../repositoryClient";

/** A remote that has not been checked for this long is reported as unchecked. */
export const REMOTE_STALE_MS = 24 * 60 * 60_000;
export const refPath = (name: string) =>
  name.split("/").map(encodeURIComponent).join("/");
export const authors = (commits: { author: string }[]) => {
  const names = [...new Set(commits.map((commit) => commit.author))];
  return names.length > 2
    ? `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`
    : names.join(" and ");
};

/** Repository: what is not committed, not shared, or not yet pulled. */
export function repositoryBlock(
  signals: ProjectSignals,
  now: Date,
): OverviewBlock | null {
  const repository = signals.repository;
  if (!repository) return null;
  const rows: OverviewRow[] = [];
  const { remote, branch, commits, incoming } = repository;
  const host = remoteLabel(remote);
  const last = commits[0];
  rows.push({
    id: "identity",
    text: remote
      ? remote.repo
        ? `${remote.owner ? `${remote.owner}/` : ""}${remote.repo}`
        : `Remote ${remote.name}`
      : "Local Git repository",
    detail: [
      branch ? `branch ${branch}` : "no branch checked out",
      last
        ? `last commit ${overviewDay(last.date)}: ${clip(last.subject, 70)}`
        : "no commits yet",
    ].join(" · "),
    action: remote?.webUrl
      ? { kind: "link", label: `Open on ${host}`, url: remote.webUrl }
      : { kind: "tab", label: "Open files", tab: "files" },
  });
  if (repository.conflicts)
    rows.push({
      id: "conflicts",
      text: `${plural(repository.conflicts, "file")} ${repository.conflicts === 1 ? "has" : "have"} merge conflicts`,
      attention: true,
      lane: "needs",
      action: { kind: "tab", label: "Resolve", tab: "version-control" },
    });
  if (repository.changed || repository.untracked)
    rows.push({
      id: "uncommitted",
      text: [
        repository.changed
          ? plural(repository.changed, "uncommitted change")
          : "",
        repository.untracked
          ? plural(repository.untracked, "untracked file")
          : "",
      ]
        .filter(Boolean)
        .join(", "),
      detail: clip(repository.changedPaths.join(", "), 110),
      lane: "next",
      action: { kind: "tab", label: "Review changes", tab: "version-control" },
    });
  if (repository.behind)
    rows.push({
      id: "incoming",
      text: `${plural(repository.behind, "new commit")} on ${host}${incoming.length ? ` by ${authors(incoming)}` : ""}`,
      detail: incoming[0] ? clip(incoming[0].subject) : undefined,
      attention: true,
      action:
        remote?.webUrl &&
        remote.host === "github.com" &&
        repository.head &&
        repository.upstream
          ? {
              kind: "link",
              label: "View on GitHub",
              url: `${remote.webUrl}/compare/${repository.head}...${refPath(repository.upstream.slice(remote.name.length + 1))}`,
            }
          : { kind: "fetch", label: `Check ${host} again` },
    });
  if (remote && repository.upstream) {
    const checked = repository.fetchedAt ? time(repository.fetchedAt) : null;
    const stale = checked === null || now.getTime() - checked > REMOTE_STALE_MS;
    if (repository.ahead || stale)
      rows.push({
        id: "remote",
        text: repository.ahead
          ? `${plural(repository.ahead, "commit")} not pushed to ${host}`
          : checked === null
            ? `${host} has not been checked for new commits`
            : `${host} has not been checked since ${overviewDay(repository.fetchedAt)}`,
        detail:
          repository.ahead && checked !== null
            ? `Last checked ${overviewDay(repository.fetchedAt)}`
            : undefined,
        lane: repository.ahead ? "next" : undefined,
        action: { kind: "fetch", label: `Check ${host}` },
      });
  }
  return block("repository", "Repository", rows);
}
