import { useState } from "react";
import { open } from "@tauri-apps/plugin-shell";
import type { DeskRecord } from "../../lib/deskClient";
import type { ProjectHome } from "../../lib/projectClient";
import {
  projectIntegrations,
  type GitHubSnapshot,
} from "../../lib/projectIntegrations";
import {
  ErrorNotice,
  Field,
  Inspect,
  useAction,
} from "../research-studio/shared";

export default function GitHubCollaboration({
  workspaceId,
  head,
  data,
  onRefresh,
}: {
  workspaceId: string;
  head: string | null;
  data?: ProjectHome;
  onRefresh: () => void;
}) {
  const [snapshot, setSnapshot] = useState<DeskRecord<GitHubSnapshot> | null>(
    null,
  );
  const [title, setTitle] = useState("");
  const [base, setBase] = useState("main");
  const [body, setBody] = useState("");
  const [created, setCreated] = useState("");
  const [imported, setImported] = useState<string[]>([]);
  const { run, busy, error } = useAction();
  const stale = snapshot?.body.head !== head;
  const append = (text: string) =>
    setBody((old) => `${old}${old ? "\n\n" : ""}${text}`);
  return (
    <details className="border-b p-3 text-sm">
      <summary>GitHub collaboration</summary>
      <div className="mt-3 space-y-3">
        <p className="text-xs text-ink-muted">
          Uses your signed-in GitHub CLI. Reading collaboration does not require
          permission to publish.
        </p>
        <button
          className="ui-button"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              setSnapshot(await projectIntegrations.github(workspaceId));
            })
          }
        >
          Check pull requests and CI
        </button>
        <ErrorNotice error={error} />
        {snapshot && (
          <>
            {snapshot.body.issuesError && (
              <p role="status">
                Issues could not be read: {snapshot.body.issuesError}
              </p>
            )}
            <p className="text-xs">
              {snapshot.body.repository} · {snapshot.body.head.slice(0, 12)} ·{" "}
              {new Date(snapshot.body.checkedAt).toLocaleString()}
              {stale ? " · Local commit changed; check again" : ""}
            </p>
            <p className="text-xs text-ink-muted">{snapshot.body.scope}</p>
            {snapshot.body.checksError && (
              <p role="status">
                CI could not be read: {snapshot.body.checksError}
              </p>
            )}
            {snapshot.body.pullRequests.map((pr) => (
              <article key={pr.number} className="rounded border p-2 space-y-2">
                <button className="underline" onClick={() => void open(pr.url)}>
                  #{pr.number} {pr.title}
                </button>
                <p>
                  {pr.isDraft ? "Draft" : pr.state} ·{" "}
                  {pr.headRefOid === head
                    ? "Matches local commit"
                    : `Different commit ${pr.headRefOid.slice(0, 12)}`}
                </p>
                <Inspect
                  value={pr.statusCheckRollup}
                  label={`Checks on PR commit ${pr.headRefOid.slice(0, 12)}`}
                />
                {pr.comments.map((comment) => (
                  <details key={comment.id}>
                    <summary>
                      Comment from {comment.author?.login ?? "GitHub"}
                    </summary>
                    <p className="whitespace-pre-wrap">{comment.body}</p>
                    <button
                      className="ui-button"
                      disabled={busy || imported.includes(comment.id)}
                      onClick={() =>
                        void run(async () => {
                          await projectIntegrations.importGitHubComment(
                            workspaceId,
                            snapshot.id,
                            comment.id,
                          );
                          setImported((ids) => [...ids, comment.id]);
                          onRefresh();
                        })
                      }
                    >
                      {imported.includes(comment.id)
                        ? "Linked to local task"
                        : "Create local task"}
                    </button>
                  </details>
                ))}
              </article>
            ))}
            {snapshot.body.runs.map((check) => (
              <p key={check.id}>
                <button
                  className="underline"
                  onClick={() => void open(check.html_url)}
                >
                  {check.name}
                </button>{" "}
                · {check.conclusion ?? check.status} ·{" "}
                {check.head_sha === head
                  ? "exact local commit"
                  : `commit ${check.head_sha.slice(0, 12)}`}
              </p>
            ))}
            {!snapshot.body.runs.length && !snapshot.body.checksError && (
              <p>No workflow runs found for this commit.</p>
            )}
            {(snapshot.body.issues ?? []).map((issue) => (
              <details key={issue.id}>
                <summary>
                  Issue #{issue.number}: {issue.title}
                </summary>
                <p className="whitespace-pre-wrap">{issue.body}</p>
                <button
                  className="ui-button"
                  onClick={() => void open(issue.url)}
                >
                  Open issue
                </button>{" "}
                <button
                  className="ui-button"
                  disabled={busy || imported.includes(issue.id)}
                  onClick={() =>
                    void run(async () => {
                      await projectIntegrations.importGitHubComment(
                        workspaceId,
                        snapshot.id,
                        issue.id,
                      );
                      setImported((ids) => [...ids, issue.id]);
                      onRefresh();
                    })
                  }
                >
                  {imported.includes(issue.id)
                    ? "Linked to local task"
                    : "Create local task"}
                </button>
              </details>
            ))}
            <details>
              <summary>Prepare draft pull request</summary>
              <div className="space-y-2 mt-2">
                <p className="text-xs">
                  Publish the selected commits with Git first. Review the exact
                  description below; creating the draft sends it to{" "}
                  {snapshot.body.repository}.
                </p>
                <Field label="Pull request title">
                  <input
                    className="ui-input"
                    value={title}
                    onChange={(e) => setTitle(e.target.value)}
                  />
                </Field>
                <Field label="Destination branch">
                  <input
                    className="ui-input"
                    value={base}
                    onChange={(e) => setBase(e.target.value)}
                  />
                </Field>
                {data && (
                  <details>
                    <summary>Add local evidence to the description</summary>
                    {data.executions.slice(0, 20).map((execution) => (
                      <p key={execution.id}>
                        <button
                          className="ui-button"
                          onClick={() =>
                            append(
                              `Local check ${execution.id}: ${execution.outcome}.\nCommand: ${execution.command.join(" ")}\nInput identity: ${execution.dependencyHash}\nReceipt recorded ${execution.createdAt}. Covers declared inputs only; CI and scientific validity are separate.`,
                            )
                          }
                        >
                          Add {execution.command[0]} receipt
                        </button>
                      </p>
                    ))}
                  </details>
                )}
                <Field label="Pull request description">
                  <textarea
                    className="ui-input w-full"
                    rows={7}
                    value={body}
                    onChange={(e) => setBody(e.target.value)}
                  />
                </Field>
                <button
                  className="ui-button"
                  disabled={busy || stale || !title.trim() || !base.trim()}
                  onClick={() =>
                    void run(async () => {
                      setCreated(
                        await projectIntegrations.draftPullRequest(
                          workspaceId,
                          {
                            expectedHead: snapshot.body.head,
                            expectedRepository: snapshot.body.repository,
                            title,
                            base,
                            body,
                          },
                        ),
                      );
                    })
                  }
                >
                  Create draft on GitHub
                </button>
                {created && (
                  <p>
                    <button
                      className="underline"
                      onClick={() => void open(created)}
                    >
                      Open created draft
                    </button>
                  </p>
                )}
              </div>
            </details>
          </>
        )}
      </div>
    </details>
  );
}
