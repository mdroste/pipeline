import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PipelineConfig, ProfileSummary } from "../lib/types";
import { router } from "../lib/router";
import {
  cloneGalleryConfig,
  WORKFLOW_GALLERY,
  type WorkflowGalleryItem,
} from "../lib/workflowGallery";

interface Props {
  onInstalled: (config: PipelineConfig) => void;
}

function uniqueProfileName(base: string, profiles: ProfileSummary[]): string {
  const names = new Set(profiles.map((profile) => profile.name.toLowerCase()));
  const ids = new Set(profiles.map((profile) => profile.id));
  const slugify = (name: string) =>
    name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "");
  const available = (name: string) =>
    !names.has(name.toLowerCase()) && !ids.has(slugify(name));
  if (available(base)) return base;
  for (let copy = 2; copy < 1_000; copy += 1) {
    const candidate = `${base} (${copy})`;
    if (available(candidate)) return candidate;
  }
  return `${base} ${Date.now()}`;
}

export default function WorkflowGalleryPage({ onInstalled }: Props) {
  const [profiles, setProfiles] = useState<ProfileSummary[]>([]);
  const [category, setCategory] = useState("All");
  const [installing, setInstalling] = useState<string | null>(null);
  const [installed, setInstalled] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<ProfileSummary[]>("list_profiles")
      .then(setProfiles)
      .catch((caught) =>
        setError(caught instanceof Error ? caught.message : String(caught)),
      );
  }, []);

  const categories = useMemo(
    () => ["All", ...new Set(WORKFLOW_GALLERY.map((item) => item.category))],
    [],
  );
  const visible =
    category === "All"
      ? WORKFLOW_GALLERY
      : WORKFLOW_GALLERY.filter((item) => item.category === category);

  const install = async (item: WorkflowGalleryItem) => {
    if (installing) return;
    setInstalling(item.id);
    setInstalled(null);
    setError(null);
    let created: ProfileSummary | null = null;
    try {
      const name = uniqueProfileName(item.name, profiles);
      created = await invoke<ProfileSummary>("create_profile", { name });
      const config = cloneGalleryConfig(item);
      if (!config.orientation_schema) {
        const surveyName =
          config.extraction?.input_mode === "folder"
            ? "orientation_folder"
            : "orientation";
        const defaults = await invoke<{ schema: Record<string, unknown> }>(
          "get_orientation_defaults",
          { name: surveyName },
        );
        config.orientation_schema = defaults.schema;
      }
      await invoke("save_pipeline_config", { config, profileId: created.id });
      const activeConfig = await invoke<PipelineConfig>("switch_profile", {
        id: created.id,
      });
      setProfiles((current) => [...current, created!]);
      setInstalled(item.id);
      onInstalled(activeConfig);
    } catch (caught) {
      if (created) {
        await invoke("delete_profile", { id: created.id }).catch(
          () => undefined,
        );
      }
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setInstalling(null);
    }
  };

  return (
    <div className="h-full overflow-auto bg-white dark:bg-gray-950">
      <div className="mx-auto max-w-6xl px-8 py-8">
        <header className="max-w-3xl">
          <p className="text-xs font-semibold uppercase tracking-[0.12em] text-gray-500 dark:text-gray-400">
            Workflow gallery
          </p>
          <h1 className="mt-2 text-2xl font-semibold tracking-tight text-gray-950 dark:text-gray-50">
            Start from a reviewed workflow
          </h1>
          <p className="mt-2 text-sm leading-6 text-gray-600 dark:text-gray-400">
            Gallery workflows install as ordinary local profiles. You can
            inspect every prompt, provider, dependency, and artifact permission
            before running them.
          </p>
        </header>

        {error && (
          <div
            role="alert"
            className="mt-6 rounded-lg border border-red-200 bg-red-50 p-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/30 dark:text-red-300"
          >
            Workflow installation failed: {error}
          </div>
        )}
        {installed && (
          <div
            role="status"
            className="mt-6 flex items-center gap-3 rounded-lg border border-emerald-200 bg-emerald-50 p-3 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950/30 dark:text-emerald-300"
          >
            <span className="min-w-0 flex-1">
              Installed and activated as your current review workflow.
            </span>
            <button
              type="button"
              onClick={() => void router.navigate({ page: "pipeline" })}
              className="shrink-0 rounded-md border border-emerald-300 px-3 py-1.5 text-xs font-medium hover:bg-emerald-100 dark:border-emerald-800 dark:hover:bg-emerald-900/40"
            >
              Open in the designer
            </button>
          </div>
        )}

        <div
          className="mt-7 flex flex-wrap gap-2"
          aria-label="Workflow categories"
        >
          {categories.map((value) => (
            <button
              type="button"
              key={value}
              aria-pressed={category === value}
              onClick={() => setCategory(value)}
              className={`rounded-full px-3 py-1.5 text-xs font-medium ${
                category === value
                  ? "bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900"
                  : "bg-gray-100 text-gray-600 hover:text-gray-900 dark:bg-gray-900 dark:text-gray-400 dark:hover:text-gray-100"
              }`}
            >
              {value}
            </button>
          ))}
        </div>

        <div className="mt-5 grid gap-4 md:grid-cols-2">
          {visible.map((item) => {
            const previouslyInstalled = profiles.some(
              (profile) =>
                profile.name === item.name ||
                profile.name.startsWith(`${item.name} (`),
            );
            return (
              <article
                key={item.id}
                className="flex min-h-72 flex-col rounded-2xl border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-800 dark:bg-gray-900/40"
              >
                <div className="flex items-start gap-4">
                  <div className="min-w-0 flex-1">
                    <span className="text-[11px] font-semibold uppercase tracking-[0.1em] text-gray-500 dark:text-gray-400">
                      {item.category}
                    </span>
                    <h2 className="mt-1 text-lg font-semibold text-gray-950 dark:text-gray-50">
                      {item.name}
                    </h2>
                  </div>
                  <span className="rounded-full bg-gray-100 px-2 py-1 text-[10px] font-medium text-gray-500 dark:bg-gray-800 dark:text-gray-400">
                    {item.config.steps.length} steps
                  </span>
                </div>
                <p className="mt-3 text-sm leading-6 text-gray-600 dark:text-gray-400">
                  {item.description}
                </p>
                <dl className="mt-5 space-y-3 text-xs">
                  <div>
                    <dt className="font-medium text-gray-900 dark:text-gray-200">
                      Inputs
                    </dt>
                    <dd className="mt-0.5 text-gray-500 dark:text-gray-400">
                      {item.inputs}
                    </dd>
                  </div>
                  <div>
                    <dt className="font-medium text-gray-900 dark:text-gray-200">
                      Produces
                    </dt>
                    <dd className="mt-0.5 text-gray-500 dark:text-gray-400">
                      {item.outcome}
                    </dd>
                  </div>
                </dl>
                <div className="mt-auto flex items-center gap-3 pt-6">
                  {previouslyInstalled && (
                    <span className="text-[11px] text-gray-500 dark:text-gray-400">
                      Installed locally
                    </span>
                  )}
                  <button
                    type="button"
                    disabled={installing !== null}
                    onClick={() => void install(item)}
                    className="ml-auto rounded-lg bg-gray-900 px-3 py-2 text-xs font-medium text-white hover:bg-gray-800 disabled:cursor-wait disabled:opacity-50 dark:bg-gray-100 dark:text-gray-900 dark:hover:bg-white"
                  >
                    {installing === item.id
                      ? "Installing…"
                      : previouslyInstalled
                        ? "Install another copy"
                        : "Install workflow"}
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      </div>
    </div>
  );
}
