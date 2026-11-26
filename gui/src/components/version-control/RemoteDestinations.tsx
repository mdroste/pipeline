import { useState } from "react";
import type {
  RepositoryView,
  VersionControlAction,
} from "../../lib/versionControlClient";
import { Button } from "../../ui";
import DiffView from "./DiffView";

export default function RemoteDestinations({
  workspaceId,
  view,
  busy,
  onAct,
  onAdd,
}: {
  workspaceId: string;
  view: RepositoryView;
  busy: boolean;
  onAct: (action: VersionControlAction) => Promise<boolean>;
  onAdd: () => void;
}) {
  const remotes =
    view.remotes ?? (view.status.remote ? [view.status.remote] : []);
  const [name, setName] = useState(view.status.remote?.name ?? "");
  const [branch, setBranch] = useState(
    view.status.upstream?.startsWith(`${name}/`)
      ? view.status.upstream.slice(name.length + 1)
      : "",
  );
  const selected = remotes.find((remote) => remote.name === name);
  const [comparison, setComparison] = useState<{
    base: string;
    target: string;
  } | null>(null);
  return (
    <details className="p-3 text-xs">
      <summary>Remote destinations</summary>
      <div className="mt-2 flex flex-wrap items-end gap-2">
        <label>
          Remote
          <select
            className="vc-input"
            value={name}
            onChange={(event) => {
              setName(event.target.value);
              setBranch("");
            }}
          >
            <option value="">Choose a remote</option>
            {remotes.map((remote) => (
              <option key={remote.name} value={remote.name}>
                {remote.name} · {remote.host}/{remote.repo}
              </option>
            ))}
          </select>
        </label>
        <label>
          Destination branch
          <input
            className="vc-input"
            value={branch}
            onChange={(event) => setBranch(event.target.value)}
            placeholder={
              selected?.provider === "overleaf"
                ? "Existing main or master"
                : (view.status.branch ?? "main")
            }
          />
        </label>
        <Button
          disabled={busy || !name || !branch}
          onClick={() => void onAct({ kind: "selectRemote", name, branch })}
        >
          Select destination
        </Button>
        <Button disabled={busy} onClick={onAdd}>
          Add remote…
        </Button>
        <Button
          disabled={busy || !view.status.remote}
          onClick={() => void onAct({ kind: "fetch" })}
        >
          Check selected remote
        </Button>
        <Button
          disabled={busy || !view.status.head || !view.status.remoteHead}
          onClick={() =>
            setComparison({
              base: view.status.head!,
              target: view.status.remoteHead!,
            })
          }
        >
          Compare with selected remote
        </Button>
      </div>
      <p className="mt-2">
        Pull and publish use this destination. Adding another remote preserves
        existing addresses.
      </p>
      {selected?.provider === "overleaf" && (
        <p className="mt-2">
          Local review branches publish to Overleaf’s existing main or master
          branch. Check the remote first. Git exchange can lose tracked changes
          and comments; use an export copy when those must be preserved.
        </p>
      )}
      {comparison && (
        <div className="mt-3 max-h-96 overflow-auto">
          <p>
            Committed versions: local {comparison.base.slice(0, 10)} → remote{" "}
            {comparison.target.slice(0, 10)}. Working edits are separate.
            {(comparison.base !== view.status.head ||
              comparison.target !== view.status.remoteHead) &&
              " The destination changed; compare again before applying."}
          </p>
          <DiffView
            workspaceId={workspaceId}
            request={{ kind: "compare", ...comparison }}
            stamp={comparison.target}
          />
        </div>
      )}
    </details>
  );
}
