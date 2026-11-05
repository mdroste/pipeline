import { useCallback, useEffect, useState } from "react";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { Button, Spinner } from "../../ui";
import {
  commitUrl,
  remoteLabel,
  type RepositoryRemote,
} from "../../lib/repositoryClient";
import {
  versionControlClient,
  type CommitDetail,
  type CommitFile,
  type HistoryCommit,
} from "../../lib/versionControlClient";
import { workbenchErrorMessage } from "../../lib/workbenchError";
import DiffView from "./DiffView";
import { FileName, StatusMark } from "./fileRows";

const when = new Intl.DateTimeFormat(undefined, {
  dateStyle: "medium",
  timeStyle: "short",
});
const day = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
const format = (date: string, formatter: Intl.DateTimeFormat) => {
  const parsed = new Date(date);
  return Number.isNaN(parsed.getTime()) ? date : formatter.format(parsed);
};

function Detail({
  workspaceId,
  sha,
  remote,
  onError,
}: {
  workspaceId: string;
  sha: string;
  remote: RepositoryRemote | null;
  onError: (message: string) => void;
}) {
  const [detail, setDetail] = useState<CommitDetail | null>(null);
  const [file, setFile] = useState<CommitFile | null>(null);
  useEffect(() => {
    let stale = false;
    setDetail(null);
    setFile(null);
    versionControlClient
      .commit(workspaceId, sha)
      .then((next) => {
        if (stale) return;
        setDetail(next);
        setFile(next.files[0] ?? null);
      })
      .catch((cause) => {
        if (!stale) onError(workbenchErrorMessage(cause));
      });
    return () => {
      stale = true;
    };
  }, [workspaceId, sha, onError]);
  if (!detail)
    return (
      <div className="vc-diff-message">
        <Spinner label="Loading" />
      </div>
    );
  const [subject, ...body] = detail.message.split("\n");
  const url = commitUrl(remote, detail.sha);
  return (
    <>
      <header className="vc-commit-head">
        <div className="min-w-0 flex-1">
          <h2>{subject}</h2>
          {body.join("\n").trim() && (
            <p className="vc-commit-body">{body.join("\n").trim()}</p>
          )}
          <p>
            {detail.author} · {format(detail.date, when)} ·{" "}
            <code>{detail.sha.slice(0, 7)}</code>
          </p>
        </div>
        {url && (
          <Button onClick={() => void openExternal(url)}>
            Open on {remoteLabel(remote)}
          </Button>
        )}
      </header>
      {detail.files.length ? (
        <>
          <ul className="vc-commit-files" aria-label="Files in this commit">
            {detail.files.map((entry) => (
              <li key={entry.path}>
                <button
                  type="button"
                  className={`vc-file-open${entry.path === file?.path ? " is-selected" : ""}`}
                  aria-current={entry.path === file?.path ? "true" : undefined}
                  onClick={() => setFile(entry)}
                >
                  <StatusMark status={entry.status} />
                  <FileName path={entry.path} />
                </button>
              </li>
            ))}
          </ul>
          {detail.filesTruncated && (
            <p className="vc-list-note">
              Showing the first {detail.files.length.toLocaleString()} files.
            </p>
          )}
        </>
      ) : (
        <p className="vc-diff-message">
          This commit changed nothing in the project folder.
        </p>
      )}
      {file && (
        <DiffView
          workspaceId={workspaceId}
          stamp={detail.sha}
          request={{
            kind: "commit",
            sha: detail.sha,
            path: file.path,
            oldPath: file.oldPath,
          }}
        />
      )}
    </>
  );
}

export default function HistoryPane({
  workspaceId,
  remote,
  stamp,
  onError,
}: {
  workspaceId: string;
  remote: RepositoryRemote | null;
  /** Changes when the repository may have new commits. */
  stamp: unknown;
  onError: (message: string) => void;
}) {
  const [commits, setCommits] = useState<HistoryCommit[] | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const load = useCallback(
    async (skip: number) => {
      try {
        const page = await versionControlClient.history(workspaceId, skip);
        setCommits((current) =>
          skip ? [...(current ?? []), ...page.commits] : page.commits,
        );
        setHasMore(page.hasMore);
      } catch (cause) {
        onError(workbenchErrorMessage(cause));
        setCommits((current) => current ?? []);
      }
    },
    [workspaceId, onError],
  );
  useEffect(() => {
    void load(0);
  }, [load, stamp]);
  const current =
    commits?.find((commit) => commit.sha === selected) ?? commits?.[0] ?? null;
  return (
    <>
      <aside className="vc-list" aria-label="Commits">
        {commits === null ? (
          <div className="vc-diff-message">
            <Spinner label="Loading" />
          </div>
        ) : commits.length === 0 ? (
          <p className="vc-list-empty">No commits yet.</p>
        ) : (
          <ul className="vc-commits">
            {commits.map((commit) => (
              <li key={commit.sha}>
                <button
                  type="button"
                  className={`vc-commit-row${commit.sha === current?.sha ? " is-selected" : ""}`}
                  aria-current={
                    commit.sha === current?.sha ? "true" : undefined
                  }
                  onClick={() => setSelected(commit.sha)}
                >
                  <span className="vc-commit-subject">{commit.subject}</span>
                  <span className="vc-commit-meta">
                    {commit.author} · {format(commit.date, day)}
                    {commit.unpushed && (
                      <span className="vc-pill">Not pushed</span>
                    )}
                    {commit.refs
                      .filter((name) => !name.includes("/"))
                      .slice(0, 2)
                      .map((name) => (
                        <span key={name} className="vc-pill">
                          {name}
                        </span>
                      ))}
                  </span>
                </button>
              </li>
            ))}
            {hasMore && (
              <li className="vc-list-note">
                <Button onClick={() => void load(commits.length)}>
                  Show older commits
                </Button>
              </li>
            )}
          </ul>
        )}
      </aside>
      <section className="vc-detail" aria-label="Commit details">
        {current && (
          <Detail
            key={current.sha}
            workspaceId={workspaceId}
            sha={current.sha}
            remote={remote}
            onError={onError}
          />
        )}
      </section>
    </>
  );
}
