import { useEffect, useRef, useState } from "react";
import type { Workspace } from "../lib/workbenchTypes";
import { workspaceClient } from "../lib/workbench/workspaceClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import Button from "../ui/Button";

export default function AttachProjectPicker({
  onMove,
  onCreated,
  onBusy,
  moveError,
}: {
  onMove: (projectId: string) => Promise<boolean>;
  onCreated: (project: Workspace) => void;
  onBusy: (busy: boolean) => void;
  moveError?: string | null;
}) {
  const [projects, setProjects] = useState<Workspace[]>([]);
  const [target, setTarget] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const active = useRef(true);
  const busyRef = useRef(false);
  const busyCallback = useRef(onBusy);
  busyCallback.current = onBusy;
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
      if (busyRef.current) busyCallback.current(false);
    };
  }, []);
  useEffect(() => {
    let live = true;
    setLoading(true);
    setError(null);
    void workspaceClient
      .listWorkspaces(false)
      .then(({ workspaces }) => {
        if (live) setProjects(workspaces);
      })
      .catch((cause) => {
        if (live) setError(workbenchErrorMessage(cause));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [attempt]);
  const proceed = async (create: boolean) => {
    if (busyRef.current || (create ? !name.trim() : !target)) return;
    busyRef.current = true;
    setBusy(true);
    onBusy(true);
    setError(null);
    try {
      let id = target;
      if (create) {
        const { record } = await workspaceClient.createWorkspace({
          name: name.trim(),
          operationId: `attach-project-${crypto.randomUUID()}`,
        });
        setProjects((old) => [record, ...old]);
        setTarget(record.id);
        setName("");
        onCreated(record);
        id = record.id;
      }
      if (!(await onMove(id)))
        setError("Couldn't move this chat. Choose the project and try again.");
    } catch (cause) {
      setError(workbenchErrorMessage(cause));
    } finally {
      busyRef.current = false;
      if (active.current) {
        setBusy(false);
        onBusy(false);
      }
    }
  };
  return (
    <div className="space-y-3 text-ui-meta">
      <p>
        Choose where to save these files. This chat and your draft move with
        you.
      </p>
      <p className="text-ink-muted">
        The chat will use the project's settings and saved context.
      </p>
      {loading ? (
        <p role="status">Loading projects…</p>
      ) : (
        <>
          <label className="block">
            Project
            <select
              className="mt-1 block w-full rounded-lg border border-line bg-surface p-2"
              disabled={busy}
              value={target}
              onChange={(event) => setTarget(event.target.value)}
            >
              <option value="">Choose a project</option>
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
          <Button
            disabled={busy || !target}
            onClick={() => void proceed(false)}
          >
            {busy ? "Moving chat…" : "Move chat and continue"}
          </Button>
          <div className="border-t border-line pt-3">
            <label className="block">
              New project name
              <input
                className="mt-1 block w-full rounded-lg border border-line bg-surface p-2"
                value={name}
                maxLength={200}
                disabled={busy}
                onChange={(event) => setName(event.target.value)}
              />
            </label>
            <Button
              className="mt-2"
              disabled={busy || !name.trim()}
              onClick={() => void proceed(true)}
            >
              Create project and continue
            </Button>
          </div>
        </>
      )}
      {(error || moveError) && (
        <div role="alert">
          <p className="break-words">{moveError || error}</p>
          <button
            disabled={busy}
            className="mt-1 underline"
            onClick={() => setAttempt((n) => n + 1)}
          >
            Reload projects
          </button>
        </div>
      )}
    </div>
  );
}
