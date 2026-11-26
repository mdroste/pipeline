import { useEffect, useMemo, useState } from "react";
import { workflowProfileClient } from "../lib/workflowProfileClient";
import { workflowSchemaClient } from "../lib/workflowSchemaClient";
import type { PipelineConfig, ProfileSummary } from "../lib/workflowTypes";
import { router } from "../lib/router";
import {
  cloneGalleryConfig,
  WORKFLOW_GALLERY,
  type WorkflowGalleryItem,
} from "../lib/workflowGallery";
import PageHeader from "../ui/PageHeader";
import ReportIssueLink from "./ReportIssueLink";

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
    workflowProfileClient
      .profiles()
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
      created = await workflowProfileClient.create({ name });
      const config = cloneGalleryConfig(item);
      if (!config.orientation_schema) {
        const surveyName =
          config.extraction?.input_mode === "folder"
            ? "orientation_folder"
            : "orientation";
        const defaults = await workflowSchemaClient.orientationDefaults({
          name: surveyName,
        });
        config.orientation_schema = defaults.schema;
      }
      await workflowProfileClient.save({ config, profileId: created.id });
      const activeConfig = await workflowProfileClient.switch({
        id: created.id,
      });
      setProfiles((current) => [...current, created!]);
      setInstalled(item.id);
      onInstalled(activeConfig);
    } catch (caught) {
      if (created) {
        await workflowProfileClient
          .remove({ id: created.id })
          .catch(() => undefined);
      }
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setInstalling(null);
    }
  };

  return (
    <div className="ui-page ui-page--index">
      <PageHeader
        title="Review design gallery"
        description="Start from a review design. Add a review design you can edit. Check its prompts, models, step order, and file access before running it."
      />

      {error && (
        <div role="alert" className="mb-6 ui-notice ui-notice--error">
          Review design installation failed: {error}
          <ReportIssueLink />
        </div>
      )}
      {installed && (
        <div
          role="status"
          className="mb-6 flex items-center gap-3 rounded-lg border border-success bg-success-soft p-3 text-sm text-success"
        >
          <span className="min-w-0 flex-1">
            Installed and activated as your current review design.
          </span>
          <button
            type="button"
            onClick={() => void router.navigate({ page: "pipeline" })}
            className="shrink-0 rounded-md border border-line-strong bg-surface px-3 py-1.5 text-xs font-medium hover:bg-sunken dark:bg-transparent text-success border-success hover:bg-success-soft"
          >
            Open in the designer
          </button>
        </div>
      )}

      <div className="ui-page-toolbar">
        <div className="ui-page-tabs" aria-label="Review design categories">
          {categories.map((value) => (
            <button
              type="button"
              key={value}
              aria-pressed={category === value}
              onClick={() => setCategory(value)}
            >
              {value}
            </button>
          ))}
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        {visible.map((item) => {
          const previouslyInstalled = profiles.some(
            (profile) =>
              profile.name === item.name ||
              profile.name.startsWith(`${item.name} (`),
          );
          return (
            <article
              key={item.id}
              // Guided setup links focus a template card by this id.
              id={`gallery-${item.id}`}
              tabIndex={-1}
              aria-labelledby={`gallery-${item.id}-name`}
              className="ui-page-card flex min-h-72 flex-col outline-none focus-visible:ring-2 focus-visible:ring-gray-400"
            >
              <div className="flex items-start gap-4">
                <div className="min-w-0 flex-1">
                  <span className="text-[0.6875rem] font-semibold uppercase tracking-[0.1em] text-ink-muted">
                    {item.category}
                  </span>
                  <h2
                    id={`gallery-${item.id}-name`}
                    className="mt-1 text-lg font-semibold"
                  >
                    {item.name}
                  </h2>
                </div>
                <span className="rounded-full bg-gray-100 px-2 py-1 text-[0.625rem] font-medium text-gray-500 dark:bg-gray-800 dark:text-gray-400">
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
                  <dd className="mt-0.5 text-ink-muted">{item.inputs}</dd>
                </div>
                <div>
                  <dt className="font-medium text-gray-900 dark:text-gray-200">
                    Produces
                  </dt>
                  <dd className="mt-0.5 text-ink-muted">{item.outcome}</dd>
                </div>
              </dl>
              <div className="mt-auto flex items-center gap-3 pt-6">
                {previouslyInstalled && (
                  <span className="text-[0.6875rem] text-ink-muted">
                    Installed locally
                  </span>
                )}
                <button
                  type="button"
                  disabled={installing !== null}
                  onClick={() => void install(item)}
                  className="ui-page-button primary ml-auto"
                >
                  {installing === item.id
                    ? "Installing…"
                    : previouslyInstalled
                      ? "Install another copy"
                      : "Install design"}
                </button>
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}
