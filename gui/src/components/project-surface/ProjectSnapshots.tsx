import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { deskClient, type DeskRecord } from "../../lib/deskClient";
import type { ProjectHome, ProjectRecord } from "../../lib/projectClient";
import { studioClient } from "../../lib/studioClient";
import {
  projectIntegrations,
  type SubmissionSnapshot,
} from "../../lib/projectIntegrations";
import {
  ErrorNotice,
  Field,
  Inspect,
  useAction,
} from "../research-studio/shared";

const toggle = (values: string[], key: string) =>
  values.includes(key)
    ? values.filter((value) => value !== key)
    : [...values, key];
export default function ProjectSnapshots({
  workspaceId,
  data,
}: {
  workspaceId: string;
  data: ProjectHome;
}) {
  const [snapshots, setSnapshots] = useState<DeskRecord<SubmissionSnapshot>[]>(
    [],
  );
  const [title, setTitle] = useState("");
  const [files, setFiles] = useState<string[]>([]);
  const [revisions, setRevisions] = useState<string[]>([]);
  const [runs, setRuns] = useState<string[]>([]);
  const [evidence, setEvidence] = useState<ProjectRecord<unknown>[]>([]);
  const [selectedEvidence, setSelectedEvidence] = useState<string[]>([]);
  const [handoff, setHandoff] = useState(
    () =>
      `## Question\n\n${data.notes
        .filter((note) => note.state === "accepted" && note.kind === "question")
        .map((note) => note.body)
        .join("\n\n")}\n\n## Accepted decisions\n\n${data.notes
        .filter((note) => note.state === "accepted" && note.kind === "decision")
        .map((note) => note.body)
        .join("\n\n")}\n\n## Remaining work\n\n${data.tasks
        .filter((task) => ["open", "investigating"].includes(task.body.status))
        .map((task) => `- ${task.body.objective}`)
        .join("\n")}\n\n## Reproduce outputs\n\n`,
  );
  const [notice, setNotice] = useState("");
  const { run, busy, error } = useAction();
  const load = () =>
    void run(async () => {
      const [saved, builds, responses] = await Promise.all([
        deskClient.records<SubmissionSnapshot>(workspaceId, "submission"),
        studioClient.records(workspaceId, "build"),
        studioClient.records(workspaceId, "response"),
      ]);
      setSnapshots(saved);
      setEvidence([
        ...builds.filter(
          (build) =>
            (build.body as { recordType?: string }).recordType === "receipt",
        ),
        ...responses,
        ...data.applications,
      ]);
    });
  return (
    <section className="project-section space-y-3">
      <h3>Milestones and packages</h3>
      <p className="text-xs text-ink-muted">
        Retain exactly what you shared or submitted. Later edits leave these
        snapshots intact.
      </p>
      <button className="ui-button" disabled={busy} onClick={load}>
        Load snapshots and evidence
      </button>
      <ErrorNotice error={error} />
      <details>
        <summary>New snapshot</summary>
        <div className="space-y-3 mt-3">
          <Field label="Milestone name">
            <input
              className="ui-input"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Journal submission · November"
            />
          </Field>
          <fieldset>
            <legend>Saved document versions</legend>
            <p className="text-xs">
              Linked review references are included for selected versions.
              Select full review reports as files if needed.
            </p>
            {data.papers
              .filter((paper) => paper.revision)
              .map((paper) => (
                <label className="block text-sm" key={paper.paper.id}>
                  <input
                    type="checkbox"
                    checked={revisions.includes(paper.revision!.id)}
                    onChange={() =>
                      setRevisions((ids) => toggle(ids, paper.revision!.id))
                    }
                  />{" "}
                  {paper.paper.title} ·{" "}
                  {paper.revision!.contentHash.slice(0, 12)}
                  {paper.revision!.inputKind === "source_tree"
                    ? " (reference only; select source files below)"
                    : ""}
                </label>
              ))}
          </fieldset>
          <details>
            <summary>Select saved working files</summary>
            <p className="text-xs">
              Choose manuscript, appendix, response letter, bibliography, and
              code as needed. Unsaved buffers are excluded.
            </p>
            <div className="max-h-64 overflow-auto">
              {data.inventory?.body.files
                .filter((file) => file.hash)
                .map((file) => (
                  <label className="block text-sm" key={file.path}>
                    <input
                      type="checkbox"
                      checked={files.includes(file.path)}
                      onChange={() =>
                        setFiles((paths) => toggle(paths, file.path))
                      }
                    />{" "}
                    {file.path} · {file.hash?.slice(0, 10)}
                  </label>
                ))}
            </div>
          </details>
          <details>
            <summary>Select evidence and checks</summary>
            {evidence.map((record) => (
              <label className="block text-sm" key={record.id}>
                <input
                  type="checkbox"
                  checked={selectedEvidence.includes(record.id)}
                  onChange={() =>
                    setSelectedEvidence((ids) => toggle(ids, record.id))
                  }
                />{" "}
                {record.kind} · {record.id} · v{record.revision}
                <Inspect
                  value={record.body}
                  label="Inspect included evidence"
                />
              </label>
            ))}
            {data.executions
              .filter(
                (execution) =>
                  !["running", "queued"].includes(execution.outcome),
              )
              .map((execution) => (
                <label className="block text-sm" key={execution.id}>
                  <input
                    type="checkbox"
                    checked={runs.includes(execution.id)}
                    onChange={() => setRuns((ids) => toggle(ids, execution.id))}
                  />{" "}
                  {execution.command.join(" ")} · {execution.outcome}
                  <Inspect
                    value={{
                      inputs: execution.inputManifest,
                      outputs: execution.outputManifest,
                      validation: execution.validation,
                    }}
                    label="Inspect included check metadata"
                  />
                </label>
              ))}
          </details>
          <Field label="Handoff note">
            <textarea
              className="ui-input w-full"
              rows={10}
              value={handoff}
              onChange={(event) => setHandoff(event.target.value)}
            />
          </Field>
          <p className="text-xs">
            Includes {files.length} working files, {revisions.length} saved
            versions, {selectedEvidence.length} evidence records and{" "}
            {runs.length} checks, plus README.md, evidence.json, and the hash
            manifest. Inspect selected metadata before sharing.
          </p>
          <button
            className="ui-button"
            disabled={busy || !title.trim()}
            onClick={() =>
              void run(async () => {
                const chosen = Object.fromEntries(
                  files.map((path) => [
                    path,
                    data.inventory?.body.files.find(
                      (file) => file.path === path,
                    )?.hash ?? "",
                  ]),
                );
                const snapshot = await projectIntegrations.captureSubmission(
                  workspaceId,
                  {
                    title,
                    handoff,
                    files: chosen,
                    revisions,
                    executions: runs,
                    records: evidence
                      .filter((record) => selectedEvidence.includes(record.id))
                      .map(({ id, kind, revision }) => ({
                        id,
                        kind,
                        revision,
                      })),
                    operationId: crypto.randomUUID(),
                  },
                );
                setSnapshots((old) => [snapshot, ...old]);
                setNotice(
                  `Saved ${snapshot.title}. Review its file manifest before export.`,
                );
              })
            }
          >
            Save immutable snapshot
          </button>
        </div>
      </details>
      {snapshots.map((snapshot) => (
        <details key={snapshot.id}>
          <summary>
            {snapshot.title} ·{" "}
            {new Date(snapshot.createdAt).toLocaleDateString()}
          </summary>
          <p className="whitespace-pre-wrap text-sm">{snapshot.body.handoff}</p>
          <ul className="text-xs my-2">
            {snapshot.body.files.map((file) => (
              <li key={file.path}>
                {file.path} · {file.size} bytes · {file.hash.slice(0, 12)}
              </li>
            ))}
          </ul>
          <button
            className="ui-button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const path = await save({
                  defaultPath: `${snapshot.title.replace(/[^a-zA-Z0-9_-]/g, "-")}.zip`,
                  filters: [{ name: "ZIP package", extensions: ["zip"] }],
                });
                if (!path) return;
                await projectIntegrations.exportSubmission(
                  workspaceId,
                  snapshot.id,
                  snapshot.contentHash,
                  path,
                );
                setNotice(`Exported ${path}`);
              })
            }
          >
            Export this snapshot
          </button>
        </details>
      ))}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
