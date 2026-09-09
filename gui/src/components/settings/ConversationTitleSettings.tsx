import { useEffect, useRef, useState } from "react";
import { workbenchClient } from "../../lib/workbenchClient";
import type {
  TitlePreferences,
  WorkspaceModelCatalog,
} from "../../lib/workbenchTypes";
import { SaveFeedback, useSettingsOperation } from "./SaveState";
const EFFORT_ORDER = ["none", "minimal", "low", "medium", "high", "xhigh"];
export default function ConversationTitleSettings() {
  const [titles, setTitles] = useState<TitlePreferences | null>(null);
  const [catalog, setCatalog] = useState<WorkspaceModelCatalog | null>(null);
  const [titlesError, setTitlesError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const sequence = useRef(0);
  const queue = useRef(Promise.resolve());
  useSettingsOperation({
    pending: saving || Boolean(titlesError && titles),
    error: titlesError,
  });
  useEffect(() => {
    let live = true;
    workbenchClient
      .titlePreferences()
      .then((v) => {
        if (live) setTitles(v);
      })
      .catch((e) => {
        if (live) setTitlesError(String(e));
      });
    workbenchClient
      .accountState(false)
      .then((account) =>
        account.status === "chatgpt" ? workbenchClient.modelCatalog() : null,
      )
      .then((value) => {
        if (live) setCatalog(value);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);
  const saveTitles = (next: TitlePreferences) => {
    setTitles(next);
    setTitlesError(null);
    setSaving(true);
    setSaved(false);
    const request = ++sequence.current;
    queue.current = queue.current
      .catch(() => {})
      .then(async () => {
        try {
          const result = await workbenchClient.saveTitlePreferences(next);
          if (request === sequence.current) {
            setTitles(result);
            setSaved(true);
          }
        } catch (e) {
          if (request === sequence.current)
            setTitlesError(`Could not save conversation titles: ${String(e)}`);
        } finally {
          if (request === sequence.current) setSaving(false);
        }
      });
  };
  return (
    <div id="conversation-titles" tabIndex={-1} className="settings-anchor">
      <section className="space-y-3 text-sm">
        <h2 className="font-semibold">Conversation titles</h2>
        <p className="text-gray-500 dark:text-neutral-400">
          Generate a short title after the first reply. This uses an additional
          model call, with the cheapest available model by default. Rename or
          regenerate titles from the conversation list.
        </p>
        {titles ? (
          <>
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={titles.enabled}
                onChange={(event) =>
                  saveTitles({ ...titles, enabled: event.target.checked })
                }
              />
              Name new conversations automatically
            </label>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block text-xs font-medium">
                Model
                <select
                  aria-label="Title model"
                  value={titles.model ?? ""}
                  disabled={!titles.enabled}
                  onChange={(event) =>
                    saveTitles({
                      ...titles,
                      model: event.target.value || null,
                      effort: null,
                    })
                  }
                  className="mt-1 w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm font-normal dark:border-neutral-700 dark:bg-neutral-950"
                >
                  <option value="">Cheapest available</option>
                  {catalog?.models.map((model) => (
                    <option key={model.id} value={model.model}>
                      {model.displayName}
                    </option>
                  ))}
                  {titles.model &&
                    !catalog?.models.some(
                      (model) => model.model === titles.model,
                    ) && (
                      <option value={titles.model}>
                        {titles.model} (not currently available)
                      </option>
                    )}
                </select>
              </label>
              <label className="block text-xs font-medium">
                Reasoning effort
                <select
                  aria-label="Title reasoning effort"
                  value={titles.effort ?? ""}
                  disabled={!titles.enabled}
                  onChange={(event) =>
                    saveTitles({
                      ...titles,
                      effort: event.target.value || null,
                    })
                  }
                  className="mt-1 w-full rounded-md border border-gray-300 bg-white px-2 py-1.5 text-sm font-normal dark:border-neutral-700 dark:bg-neutral-950"
                >
                  <option value="">Lowest available</option>
                  {titleEffortOptions(catalog, titles).map((effort) => (
                    <option key={effort} value={effort}>
                      {effort}
                    </option>
                  ))}
                </select>
              </label>
            </div>
            {(!catalog || catalog.models.length === 0) && (
              <p className="text-xs text-gray-500">
                Model choices appear after signing in.
              </p>
            )}
          </>
        ) : (
          <p className="text-xs text-gray-500">
            {titlesError ? "" : "Loading title settings…"}
          </p>
        )}
        <SaveFeedback
          pending={saving}
          saved={saved}
          error={titlesError}
          retry={titles ? () => saveTitles(titles) : undefined}
        />
      </section>
    </div>
  );
}

/** Efforts the chosen model advertises, or every advertised effort when the
 *  choice is automatic; the saved value stays selectable while signed out. */
function titleEffortOptions(
  catalog: WorkspaceModelCatalog | null,
  titles: TitlePreferences,
): string[] {
  const models = catalog?.models ?? [];
  const pool = titles.model
    ? models.filter((model) => model.model === titles.model)
    : models;
  const advertised = new Set(
    pool.flatMap((model) =>
      model.supportedReasoningEfforts.map((option) => option.reasoningEffort),
    ),
  );
  if (titles.effort) advertised.add(titles.effort);
  return [...advertised].sort(
    (left, right) => EFFORT_ORDER.indexOf(left) - EFFORT_ORDER.indexOf(right),
  );
}
