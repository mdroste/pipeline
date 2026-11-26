import { useState } from "react";
import { deskClient, type DeskRecord } from "../../lib/deskClient";
import { studioClient } from "../../lib/studioClient";
import {
  projectIntegrations,
  type CheckPolicy,
} from "../../lib/projectIntegrations";
import {
  ErrorNotice,
  Field,
  Inspect,
  useAction,
} from "../research-studio/shared";

export default function ProjectChecks({
  workspaceId,
}: {
  workspaceId: string;
}) {
  const [policy, setPolicy] = useState<CheckPolicy>({
    enabled: false,
    sourcePath: "main.tex",
    bibliographies: ["references.bib"],
  });
  const [revision, setRevision] = useState<number | null>(null);
  const [history, setHistory] = useState<DeskRecord[]>([]);
  const [notice, setNotice] = useState("");
  const { run, busy, error } = useAction();
  const load = async () => {
    const [policies, receipts] = await Promise.all([
      studioClient.records(workspaceId, "check_policy"),
      deskClient.records(workspaceId, "check_receipt"),
    ]);
    if (policies[0]) setPolicy(policies[0].body);
    setRevision(policies[0]?.revision ?? 0);
    setHistory(receipts);
  };
  return (
    <section className="project-section space-y-3">
      <h3>Research checks</h3>
      <p className="text-xs text-ink-muted">
        Optional local citation validation. Builds, analyses, and model reviews
        still use their existing launch and cancellation controls.
      </p>
      <button
        className="ui-button"
        disabled={busy}
        onClick={() => void run(load)}
      >
        Configure checks and view history
      </button>
      <ErrorNotice error={error} />
      {revision !== null && (
        <>
          <Field label="Checked TeX source">
            <input
              className="ui-input"
              value={policy.sourcePath}
              onChange={(event) =>
                setPolicy({ ...policy, sourcePath: event.target.value })
              }
            />
          </Field>
          <Field label="Bibliographies (one project path per line)">
            <textarea
              className="ui-input"
              value={policy.bibliographies.join("\n")}
              onChange={(event) =>
                setPolicy({
                  ...policy,
                  bibliographies: event.target.value.split("\n"),
                })
              }
            />
          </Field>
          <label className="block text-sm">
            <input
              type="checkbox"
              checked={policy.enabled}
              onChange={(event) =>
                setPolicy({ ...policy, enabled: event.target.checked })
              }
            />{" "}
            Check after saving one of these files in Pipeline
          </label>
          <p className="text-xs">
            Parser v1 checks missing and duplicate keys in the selected source
            and bibliographies. Changes settle for 500 ms, identical inputs
            reuse their receipt, and generated outputs do not trigger checks.
            Opening a project launches nothing. No network or model is used.
          </p>
          <button
            className="ui-button"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                const saved = await projectIntegrations.saveCheckPolicy(
                  workspaceId,
                  revision,
                  {
                    ...policy,
                    bibliographies: policy.bibliographies
                      .map((path) => path.trim())
                      .filter(Boolean),
                  },
                );
                setPolicy(saved.body);
                setRevision(saved.revision);
                setNotice(
                  "Check settings saved. Saving settings does not run a check.",
                );
              })
            }
          >
            Save check settings
          </button>{" "}
          <button
            className="ui-button"
            disabled={busy || revision === 0}
            onClick={() =>
              void run(async () => {
                await projectIntegrations.runCheck(workspaceId);
                await load();
                setNotice(
                  "Checked the saved files using the saved configuration.",
                );
              })
            }
          >
            Check saved files now
          </button>
        </>
      )}
      {history.map((receipt) => (
        <details key={receipt.id}>
          <summary>
            {String(receipt.body.outcome)} ·{" "}
            {new Date(receipt.createdAt).toLocaleString()}
          </summary>
          <Inspect value={receipt.body} label="Input versions and result" />
        </details>
      ))}
      {notice && <p role="status">{notice}</p>}
    </section>
  );
}
