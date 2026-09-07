import { useCallback, useEffect, useState } from "react";
import { workbenchClient } from "../lib/workbenchClient";
import { workbenchErrorMessage } from "../lib/workbenchError";
import type {
  ConversationSnapshot,
  RecipeInputCheck,
  RecipeRun,
  ResearchExecution,
  ResearchRecipe,
} from "../lib/workbenchTypes";

function operation(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`;
}

function lines(value: string) {
  return value.split("\n").map((item) => item.trim()).filter(Boolean);
}

interface WorkspaceRecipesPanelProps {
  snapshot: ConversationSnapshot;
  onSnapshot: (snapshot: ConversationSnapshot) => void;
  onError: (message: string) => void;
}

export default function WorkspaceRecipesPanel({
  snapshot,
  onSnapshot,
  onError,
}: WorkspaceRecipesPanelProps) {
  const workspaceId = snapshot.session.workspaceId;
  const [recipes, setRecipes] = useState<ResearchRecipe[]>([]);
  const [runs, setRuns] = useState<RecipeRun[]>([]);
  const [executions, setExecutions] = useState<ResearchExecution[]>([]);
  const [checks, setChecks] = useState<RecipeInputCheck[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [recordedChecks, setRecordedChecks] = useState<Record<string, string[]>>({});
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    if (!workspaceId) {
      setRecipes([]);
      setRuns([]);
      setExecutions([]);
      setSelectedId("");
      return;
    }
    const [nextRecipes, nextRuns, nextExecutions] = await Promise.all([
      workbenchClient.listRecipes(workspaceId),
      workbenchClient.listRecipeRuns(snapshot.session.id),
      workbenchClient.listExecutions(workspaceId),
    ]);
    setRecipes(nextRecipes);
    setRuns(nextRuns);
    setExecutions(nextExecutions);
    const activeRecipe = typeof snapshot.session.overrides.recipeId === "string"
      ? snapshot.session.overrides.recipeId
      : "";
    setSelectedId((current) => {
      if (nextRecipes.some((recipe) => recipe.id === current)) return current;
      if (nextRecipes.some((recipe) => recipe.id === activeRecipe)) return activeRecipe;
      return nextRecipes[0]?.id ?? "";
    });
  }, [snapshot.session.id, snapshot.session.overrides.recipeId, workspaceId]);

  useEffect(() => {
    void load().catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [load, onError]);

  useEffect(() => {
    if (!workspaceId || !selectedId) {
      setChecks([]);
      return;
    }
    void workbenchClient.checkRecipeInputs(snapshot.session.id, selectedId)
      .then(setChecks)
      .catch((cause) => onError(workbenchErrorMessage(cause)));
  }, [onError, selectedId, snapshot.session.id, workspaceId]);

  const act = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      await load();
    } catch (cause) {
      onError(workbenchErrorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const patchSession = async (overrides: Record<string, unknown>) => {
    const latest = await workbenchClient.conversationSnapshot(snapshot.session.id);
    const updated = await workbenchClient.updateSession({
      sessionId: latest.session.id,
      expectedRevision: latest.session.revision,
      operationId: operation("recipe-session"),
      overrides,
    });
    onSnapshot({ ...latest, session: updated.record, sequence: updated.sequence });
  };

  const selected = recipes.find((recipe) => recipe.id === selectedId);

  if (!workspaceId) {
    return <p className="text-xs text-amber-700">Move this conversation into a Workspace to use research recipes.</p>;
  }

  return <div className="space-y-4">
    <label className="block">
      <span className="text-xs font-medium">Recipe</span>
      <select
        value={selectedId}
        onChange={(event) => setSelectedId(event.target.value)}
        className="mt-1 w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950"
      >
        {recipes.map((recipe) => <option key={recipe.id} value={recipe.id}>
          {recipe.name} · v{recipe.version}{recipe.builtIn ? "" : " · custom"}
        </option>)}
      </select>
    </label>

    {selected && <div className="rounded border bg-white p-3 dark:bg-neutral-950">
      <p className="text-xs">{selected.description}</p>
      <div className="mt-2 flex flex-wrap gap-1">
        {selected.expectedChecks.map((check) => <span key={check} className="rounded bg-gray-100 px-2 py-1 text-[10px] dark:bg-neutral-800">
          {check.replaceAll("_", " ")}
        </span>)}
      </div>
    </div>}

    <div className="space-y-1">
      {checks.map((check) => <div key={check.input} className={`rounded border p-2 text-xs ${check.available ? "border-emerald-200 text-emerald-700" : "border-amber-200 text-amber-700"}`}>
        <span className="font-medium">{check.available ? "Ready" : "Missing"}: {check.input.replaceAll("_", " ")}</span>
        <span className="block text-[11px] opacity-80">{check.detail}</span>
      </div>)}
    </div>

    <div className="flex flex-wrap gap-2">
      <button
        type="button"
        disabled={!selected || busy}
        onClick={() => void act(async () => {
          const nextChecks = await workbenchClient.checkRecipeInputs(snapshot.session.id, selected!.id);
          setChecks(nextChecks);
          await patchSession({
            ...snapshot.session.overrides,
            recipeId: selected!.id,
            mode: selected!.suggestedPermissionMode,
          });
          await workbenchClient.startRecipe({
            sessionId: snapshot.session.id,
            recipeId: selected!.id,
            operationId: operation("recipe-run"),
          });
        })}
        className="rounded bg-gray-900 px-3 py-2 text-xs text-white dark:bg-neutral-100 dark:text-neutral-900"
      >
        Use in this conversation
      </button>
      <button
        type="button"
        disabled={!selected || busy}
        onClick={() => void act(async () => {
          const name = window.prompt("Name for editable recipe", `${selected!.name} copy`)?.trim();
          if (!name) return;
          const created = await workbenchClient.cloneRecipe({
            workspaceId,
            sourceRecipeId: selected!.id,
            name,
          });
          setSelectedId(created.id);
        })}
        className="rounded border px-3 py-2 text-xs"
      >
        Clone and edit
      </button>
      <button
        type="button"
        disabled={busy || !snapshot.session.overrides.recipeId}
        onClick={() => void act(async () => {
          const { recipeId: _removed, ...overrides } = snapshot.session.overrides;
          await patchSession(overrides);
        })}
        className="rounded border px-3 py-2 text-xs"
      >
        Disable recipe
      </button>
    </div>

    {selected && !selected.builtIn && <details>
      <summary className="cursor-pointer text-xs font-semibold">Edit custom recipe</summary>
      <form className="mt-2 space-y-2" onSubmit={(event) => {
        event.preventDefault();
        const data = new FormData(event.currentTarget);
        void act(async () => {
          await workbenchClient.updateRecipe({
            recipeId: selected.id,
            expectedRevision: selected.revision,
            name: String(data.get("name")),
            description: String(data.get("description")),
            instructions: String(data.get("instructions")),
            requiredInputs: lines(String(data.get("requiredInputs"))),
            requiredTools: lines(String(data.get("requiredTools"))),
            suggestedPermissionMode: String(data.get("mode")) as "inspect" | "edit",
            expectedChecks: lines(String(data.get("expectedChecks"))),
          });
        });
      }}>
        <input name="name" defaultValue={selected.name} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <textarea name="description" defaultValue={selected.description} rows={2} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <textarea name="instructions" defaultValue={selected.instructions} rows={7} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <textarea name="requiredInputs" defaultValue={selected.requiredInputs.join("\n")} rows={3} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <textarea name="requiredTools" defaultValue={selected.requiredTools.join("\n")} rows={3} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <textarea name="expectedChecks" defaultValue={selected.expectedChecks.join("\n")} rows={4} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950" />
        <select name="mode" defaultValue={selected.suggestedPermissionMode} className="w-full rounded border bg-white p-2 text-xs dark:bg-neutral-950">
          <option value="inspect">Inspect</option>
          <option value="edit">Edit</option>
        </select>
        <button type="submit" className="rounded border px-3 py-2 text-xs">Save recipe version</button>
      </form>
    </details>}

    <section>
      <h3 className="text-xs font-semibold uppercase tracking-wide text-gray-500">Completion cards</h3>
      <div className="mt-2 space-y-2">
        {runs.map((run) => <div key={run.id} className="rounded border bg-white p-3 dark:bg-neutral-950">
          <div className="flex items-center gap-2">
            <span className="font-medium">{run.recipe.name}</span>
            <span className={`text-[11px] ${run.status === "completed" ? "text-emerald-600" : run.status === "incomplete" ? "text-amber-600" : "text-blue-600"}`}>{run.status}</span>
          </div>
          <p className="mt-1 text-[11px] text-gray-500">
            {run.artifacts.length} artifact(s) · {run.checks.length} check record(s) · {run.unresolvedIssues.length} unresolved · {run.missingEvidence.length} missing evidence
          </p>
          {run.status === "active" && <div className="mt-2 space-y-2">
            {run.recipe.expectedChecks.map((check) => <label key={check} className="block text-[11px]">
              <input
                type="checkbox"
                checked={(recordedChecks[run.id] ?? []).includes(check)}
                onChange={(event) => setRecordedChecks((current) => {
                  const prior = current[run.id] ?? [];
                  return {
                    ...current,
                    [run.id]: event.target.checked
                      ? [...prior, check]
                      : prior.filter((item) => item !== check),
                  };
                })}
                className="mr-2"
              />
              {check.replaceAll("_", " ")} actually checked
            </label>)}
            <button
              type="button"
              disabled={busy}
              onClick={() => void act(async () => {
                const unresolved = lines(window.prompt("Unresolved issues, one per line", "") ?? "");
                const completed = new Set(recordedChecks[run.id] ?? []);
                const currentInputs = await workbenchClient.checkRecipeInputs(snapshot.session.id, run.recipe.id);
                const checkRecords = run.recipe.expectedChecks.map((name) => ({
                  name,
                  status: completed.has(name) ? "recorded" : "not_recorded",
                }));
                const missingEvidence = [
                  ...currentInputs.filter((input) => !input.available).map((input) => input.detail),
                  ...run.recipe.expectedChecks.filter((name) => !completed.has(name)).map((name) => `Check not recorded: ${name.replaceAll("_", " ")}`),
                ];
                await workbenchClient.completeRecipe({
                  recipeRunId: run.id,
                  artifacts: executions
                    .filter((item) => item.sessionId === snapshot.session.id && item.outcome === "completed")
                    .map((item) => ({ executionId: item.id, outputManifest: item.outputManifest })),
                  checks: checkRecords,
                  unresolvedIssues: unresolved,
                  missingEvidence,
                });
              })}
              className="rounded border px-2 py-1 text-[11px]"
            >
              Record completion
            </button>
          </div>}
          {(run.unresolvedIssues.length > 0 || run.missingEvidence.length > 0) && <ul className="mt-2 list-disc pl-4 text-[11px] text-amber-700">
            {[...run.unresolvedIssues, ...run.missingEvidence].map((item, index) => <li key={`${item}-${index}`}>{item}</li>)}
          </ul>}
        </div>)}
      </div>
    </section>
  </div>;
}
