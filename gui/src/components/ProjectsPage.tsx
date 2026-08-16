import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Project, ProjectsResponse, RunSummary } from "../lib/types";
import ProjectIssueLedgerPanel from "./ProjectIssueLedgerPanel";
import type { ArtifactSelectionTarget } from "./ArtifactExplorer";

interface Props {
  onOpenRun: (runId: string, source?: ArtifactSelectionTarget) => void;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString();
}

function runLabel(run: RunSummary): string {
  return run.title || run.input_name || "Untitled report";
}

function sameInputLineage(left: RunSummary, right: RunSummary): boolean {
  const a = left.input_identity;
  const b = right.input_identity;
  if (a && b) {
    if (a.lineage_id && a.lineage_id === b.lineage_id) return true;
    if (a.content_hash && a.content_hash === b.content_hash) return true;
    if (a.selection_key && a.selection_key === b.selection_key) return true;
  }
  return Boolean(left.input_path && left.input_path === right.input_path);
}

export default function ProjectsPage({ onOpenRun }: Props) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [runs, setRuns] = useState<RunSummary[]>([]);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [createName, setCreateName] = useState("");
  const [createDescription, setCreateDescription] = useState("");
  const [editName, setEditName] = useState("");
  const [editDescription, setEditDescription] = useState("");
  const [addRunId, setAddRunId] = useState("");

  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      const [projectResponse, runResponse] = await Promise.all([
        invoke<ProjectsResponse>("list_projects"),
        invoke<RunSummary[]>("list_runs"),
      ]);
      setProjects(projectResponse.projects);
      setWarnings(projectResponse.warnings);
      setRuns(runResponse);
      setSelectedId((current) =>
        current && projectResponse.projects.some((project) => project.id === current)
          ? current
          : (projectResponse.projects[0]?.id ?? null),
      );
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const selected = projects.find((project) => project.id === selectedId) ?? null;
  useEffect(() => {
    setEditName(selected?.name ?? "");
    setEditDescription(selected?.description ?? "");
    setAddRunId("");
  }, [selected?.id]);

  const runsById = useMemo(
    () => new Map(runs.map((run) => [run.run_id, run])),
    [runs],
  );
  const projectRuns = selected
    ? selected.run_ids.map((runId) => ({ runId, run: runsById.get(runId) ?? null }))
    : [];
  const availableRuns = selected
    ? runs.filter((run) => !selected.run_ids.includes(run.run_id))
    : [];

  const replaceProject = (project: Project) => {
    setProjects((current) =>
      current.map((candidate) => candidate.id === project.id ? project : candidate),
    );
  };

  const createProject = async () => {
    if (!createName.trim() || pending) return;
    setPending(true);
    setError(null);
    try {
      const project = await invoke<Project>("create_project", {
        name: createName,
        description: createDescription || null,
      });
      setProjects((current) => [project, ...current]);
      setSelectedId(project.id);
      setCreateName("");
      setCreateDescription("");
      setCreating(false);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  const saveProject = async () => {
    if (!selected || !editName.trim() || pending) return;
    setPending(true);
    setError(null);
    try {
      const updated = await invoke<Project>("update_project", {
        id: selected.id,
        name: editName,
        description: editDescription,
      });
      replaceProject(updated);
      setEditName(updated.name);
      setEditDescription(updated.description);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  const setRunMembership = async (runId: string, included: boolean) => {
    if (!selected || pending) return;
    setPending(true);
    setError(null);
    try {
      replaceProject(await invoke<Project>("set_project_run", {
        projectId: selected.id,
        runId,
        included,
      }));
      setAddRunId("");
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  const addRelatedRuns = async () => {
    if (!selected || pending) return;
    const members = projectRuns.flatMap(({ run }) => run ? [run] : []);
    const related = availableRuns.filter((run) => members.some((member) => sameInputLineage(member, run)));
    if (related.length === 0) return;
    setPending(true);
    setError(null);
    try {
      let updated = selected;
      for (const run of related) {
        updated = await invoke<Project>("set_project_run", {
          projectId: selected.id,
          runId: run.run_id,
          included: true,
        });
      }
      replaceProject(updated);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  const deleteSelected = async () => {
    if (!selected || pending) return;
    if (!window.confirm(`Delete the project “${selected.name}”? Its reports will be kept.`)) return;
    setPending(true);
    setError(null);
    try {
      await invoke("delete_project", { id: selected.id });
      const remaining = projects.filter((project) => project.id !== selected.id);
      setProjects(remaining);
      setSelectedId(remaining[0]?.id ?? null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setPending(false);
    }
  };

  return (
    <div className="flex h-full min-h-0 bg-white dark:bg-gray-950">
      <aside className="flex w-72 shrink-0 flex-col border-r border-gray-200 bg-gray-50 dark:border-gray-800 dark:bg-gray-950">
        <div className="flex items-center gap-3 border-b border-gray-200 px-5 py-4 dark:border-gray-800">
          <div className="min-w-0 flex-1">
            <h1 className="text-base font-semibold text-gray-950 dark:text-gray-50">Projects</h1>
            <p className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">Track reports and findings over time.</p>
          </div>
          <button
            type="button"
            onClick={() => setCreating((value) => !value)}
            className="rounded-lg bg-gray-900 px-2.5 py-1.5 text-xs font-medium text-white dark:bg-gray-100 dark:text-gray-900"
          >
            New
          </button>
        </div>
        {creating && (
          <div className="space-y-2 border-b border-gray-200 p-4 dark:border-gray-800">
            <input
              autoFocus
              aria-label="New project name"
              value={createName}
              onChange={(event) => setCreateName(event.target.value)}
              placeholder="Project name"
              className="w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm dark:border-gray-700 dark:bg-gray-900"
            />
            <textarea
              aria-label="New project description"
              value={createDescription}
              onChange={(event) => setCreateDescription(event.target.value)}
              placeholder="Optional note"
              rows={2}
              className="w-full resize-none rounded-lg border border-gray-300 bg-white px-3 py-2 text-xs dark:border-gray-700 dark:bg-gray-900"
            />
            <div className="flex justify-end gap-2">
              <button type="button" onClick={() => setCreating(false)} className="px-2 py-1 text-xs text-gray-500">Cancel</button>
              <button
                type="button"
                disabled={!createName.trim() || pending}
                onClick={() => void createProject()}
                className="rounded-md bg-gray-900 px-2.5 py-1 text-xs text-white disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900"
              >
                Create
              </button>
            </div>
          </div>
        )}
        <div className="min-h-0 flex-1 overflow-auto p-2">
          {projects.map((project) => (
            <button
              type="button"
              key={project.id}
              onClick={() => setSelectedId(project.id)}
              className={`mb-1 w-full rounded-lg px-3 py-2.5 text-left ${
                project.id === selectedId
                  ? "bg-white shadow-sm ring-1 ring-gray-200 dark:bg-gray-800 dark:ring-gray-700"
                  : "hover:bg-white/70 dark:hover:bg-gray-900"
              }`}
            >
              <span className="block truncate text-sm font-medium text-gray-900 dark:text-gray-100">{project.name}</span>
              <span className="mt-0.5 block text-[11px] text-gray-500 dark:text-gray-400">
                {project.run_ids.length} run{project.run_ids.length === 1 ? "" : "s"} · {formatDate(project.updated)}
              </span>
            </button>
          ))}
          {!loading && projects.length === 0 && (
            <p className="px-3 py-8 text-center text-xs leading-5 text-gray-500 dark:text-gray-400">
              Create a project, then add reports from your saved history.
            </p>
          )}
        </div>
      </aside>

      <main className="min-w-0 flex-1 overflow-auto">
        {error && (
          <div role="alert" className="m-6 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300">
            {error}
          </div>
        )}
        {warnings.length > 0 && (
          <div role="status" className="mx-6 mt-6 rounded-lg border border-amber-200 bg-amber-50 p-3 text-xs text-amber-800 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
            {warnings.length} project file{warnings.length === 1 ? " was" : "s were"} skipped because it could not be read.
          </div>
        )}
        {loading ? (
          <div className="flex h-full items-center justify-center text-sm text-gray-400">Loading projects…</div>
        ) : selected ? (
          <div className="mx-auto max-w-5xl p-8">
            <div className="flex items-start gap-4">
              <div className="min-w-0 flex-1">
                <label className="text-xs font-medium text-gray-500 dark:text-gray-400" htmlFor="project-name">Project name</label>
                <input
                  id="project-name"
                  value={editName}
                  onChange={(event) => setEditName(event.target.value)}
                  className="mt-1 w-full border-0 border-b border-transparent bg-transparent px-0 py-1 text-2xl font-semibold text-gray-950 outline-none focus:border-gray-300 dark:text-gray-50 dark:focus:border-gray-700"
                />
                <textarea
                  aria-label="Project description"
                  value={editDescription}
                  onChange={(event) => setEditDescription(event.target.value)}
                  placeholder="Add a note about the work, source set, or review goal…"
                  rows={2}
                  className="mt-3 w-full resize-y rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 text-sm text-gray-700 dark:border-gray-800 dark:bg-gray-900 dark:text-gray-300"
                />
              </div>
              <div className="flex shrink-0 gap-2">
                <button
                  type="button"
                  disabled={pending || !editName.trim() || (editName === selected.name && editDescription === selected.description)}
                  onClick={() => void saveProject()}
                  className="rounded-lg border border-gray-300 px-3 py-1.5 text-xs font-medium text-gray-700 disabled:opacity-40 dark:border-gray-700 dark:text-gray-300"
                >
                  Save details
                </button>
                <button type="button" disabled={pending} onClick={() => void deleteSelected()} className="rounded-lg px-3 py-1.5 text-xs text-red-600 disabled:opacity-40 dark:text-red-400">
                  Delete project
                </button>
              </div>
            </div>

            <div className="mt-10 border-t border-gray-200 pt-8 dark:border-gray-800">
              <ProjectIssueLedgerPanel project={selected} runs={runs} onOpenRun={onOpenRun} />
            </div>

            <section className="mt-10 border-t border-gray-200 pt-8 dark:border-gray-800">
              <div className="flex items-center gap-3">
                <div>
                  <h2 className="text-sm font-semibold text-gray-900 dark:text-gray-100">Reports</h2>
                  <p className="mt-0.5 text-xs text-gray-500 dark:text-gray-400">Reports remain immutable and may belong to more than one project.</p>
                </div>
                <div className="ml-auto flex items-center gap-2">
                  {projectRuns.some(({ run }) => run) && (
                    <button
                      type="button"
                      disabled={pending || !availableRuns.some((run) => projectRuns.some(({ run: member }) => member ? sameInputLineage(member, run) : false))}
                      onClick={() => void addRelatedRuns()}
                      className="rounded-lg border border-gray-300 px-2.5 py-1.5 text-xs text-gray-600 disabled:opacity-40 dark:border-gray-700 dark:text-gray-400"
                    >
                      Add reports for same input
                    </button>
                  )}
                  <select
                    aria-label="Report to add"
                    value={addRunId}
                    onChange={(event) => setAddRunId(event.target.value)}
                    className="max-w-xs rounded-lg border border-gray-300 bg-white px-2.5 py-1.5 text-xs dark:border-gray-700 dark:bg-gray-900"
                  >
                    <option value="">Add an existing report…</option>
                    {availableRuns.map((run) => (
                      <option key={run.run_id} value={run.run_id}>{runLabel(run)} · {formatDate(run.created)}</option>
                    ))}
                  </select>
                  <button
                    type="button"
                    disabled={!addRunId || pending}
                    onClick={() => void setRunMembership(addRunId, true)}
                    className="rounded-lg bg-gray-900 px-2.5 py-1.5 text-xs font-medium text-white disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900"
                  >
                    Add
                  </button>
                </div>
              </div>

              <div className="mt-4 overflow-hidden rounded-xl border border-gray-200 dark:border-gray-800">
                {projectRuns.map(({ runId, run }, index) => (
                  <div key={runId} className={`flex items-center gap-4 px-4 py-3 ${index ? "border-t border-gray-200 dark:border-gray-800" : ""}`}>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium text-gray-900 dark:text-gray-100">{run ? runLabel(run) : runId}</p>
                      <p className="mt-0.5 truncate text-xs text-gray-500 dark:text-gray-400">
                        {run ? `${run.profile_name} · ${formatDate(run.created)} · ${run.status}` : "Report no longer exists"}
                      </p>
                    </div>
                    {run && (
                      <button type="button" onClick={() => onOpenRun(run.run_id)} className="rounded-lg border border-gray-300 px-2.5 py-1 text-xs dark:border-gray-700">Open</button>
                    )}
                    <button type="button" disabled={pending} onClick={() => void setRunMembership(runId, false)} className="px-2 py-1 text-xs text-gray-500 disabled:opacity-40">Remove</button>
                  </div>
                ))}
                {projectRuns.length === 0 && (
                  <p className="px-4 py-10 text-center text-sm text-gray-500 dark:text-gray-400">No reports have been added yet.</p>
                )}
              </div>
            </section>
          </div>
        ) : (
          <div className="flex h-full items-center justify-center text-sm text-gray-500 dark:text-gray-400">Create a project to begin.</div>
        )}
      </main>
    </div>
  );
}
