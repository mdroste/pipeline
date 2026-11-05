/**
 * Names for models and thinking levels that nobody pinned.
 *
 * An unset model is never shown as a bare "default". It shows the model that
 * will run and who picks it: the CLI (Claude Code, ChatGPT), Pipeline's
 * recommended model on an API connection, or the shared default model from
 * Settings. Recorded runs use the same names.
 */
import {
  connectionDescription,
  providerFamilies,
  reviewConnectionId,
  reviewConnectionLabel,
} from "./connections";
import type {
  ModelCatalog,
  ModelSelection,
  RosterEntry,
  Settings,
} from "./types";

/** What older builds store in place of a model when the CLI chose it. */
const UNRECORDED_MODEL = "Provider default";

const capitalize = (value: string) =>
  value ? value.charAt(0).toUpperCase() + value.slice(1) : value;

export function modelDisplayName(
  catalog: ModelCatalog | undefined,
  id: string,
): string {
  return catalog?.models.find((model) => model.id === id)?.display_name || id;
}

function connectionFor(settings: Settings, provider: string) {
  return connectionDescription(reviewConnectionId(settings, provider));
}

/**
 * Who picks the model when a stage or step sets none. Subscriptions leave it
 * to the CLI; API connections use the model Pipeline recommends.
 */
export function modelChooser(settings: Settings, provider: string): string {
  if (provider === "local") return "Local server";
  const connection = connectionFor(settings, provider);
  return connection.access === "api" ? "Pipeline" : connection.label;
}

export interface UnsetModel {
  /** The model that runs, when known. */
  id: string | null;
  name: string | null;
  /** Who picks it, for example "Claude Code" or "Pipeline". */
  chooser: string;
  /** `default` when the shared default model in Settings applies. */
  source: "default" | "local" | "chooser";
}

/** The model a provider runs when nothing more specific chose one. */
export function unsetModel(
  settings: Settings,
  provider: string,
  catalog?: ModelCatalog,
): UnsetModel {
  if (provider === "local") {
    const id = settings.local_model?.trim() || null;
    return { id, name: id, chooser: "Local server", source: "local" };
  }
  if (provider === settings.preferred_provider && settings.default_model) {
    const id = settings.default_model;
    return {
      id,
      name: modelDisplayName(catalog, id),
      chooser: "Default model",
      source: "default",
    };
  }
  const api = connectionFor(settings, provider).access === "api";
  // Mirrors model_catalog::resolve: API Automatic sends the recommended
  // model; a CLI picks its own default, which its model list names.
  const id = api
    ? catalog?.recommended_model ||
      catalog?.default_model ||
      catalog?.models[0]?.id ||
      null
    : catalog?.default_model || null;
  return {
    id,
    name: id ? modelDisplayName(catalog, id) : null,
    chooser: modelChooser(settings, provider),
    source: "chooser",
  };
}

/**
 * The first option of a model menu: who picks, then the model, as in
 * "Claude Code's choice: Opus 5.5".
 */
export function unsetModelOption(
  settings: Settings,
  provider: string,
  catalog?: ModelCatalog,
  loading = false,
): string {
  const model = unsetModel(settings, provider, catalog);
  if (model.source === "local")
    return `Local server: ${model.name ?? "no model selected"}`;
  if (model.source === "default") return `Default model: ${model.name}`;
  if (model.name) return `${model.chooser}'s choice: ${model.name}`;
  return `${model.chooser}'s choice${loading ? " (checking…)" : ""}`;
}

/**
 * A summary of an unset model: the model first, as in
 * "Opus 5.5 (Claude Code's choice)".
 */
export function unsetModelSummary(
  settings: Settings,
  provider: string,
  catalog?: ModelCatalog,
): string {
  const model = unsetModel(settings, provider, catalog);
  if (model.source === "local") return model.name ?? "No local model selected";
  if (model.source === "default") return `${model.name} (default model)`;
  return model.name
    ? `${model.name} (${model.chooser}'s choice)`
    : `${model.chooser}'s choice`;
}

/** The thinking level used when a stage or step sets none. */
export function unsetEffortLabel(settings: Settings, provider: string): string {
  if (provider === settings.preferred_provider && settings.default_effort)
    return `Default thinking: ${capitalize(settings.default_effort)}`;
  const who =
    provider === "local"
      ? "Local server"
      : connectionFor(settings, provider).label;
  return `${who} default thinking`;
}

/**
 * One provider's line in a stage summary, as in
 * "Claude · Opus 5.5 (Claude Code's choice) · High". Every part names a value.
 */
export function stageModelSummary(
  settings: Settings,
  provider: string,
  models: Record<string, ModelSelection> | undefined,
  efforts: Record<string, string> | undefined,
  catalogs: Record<string, ModelCatalog>,
): string {
  const transport =
    provider === "local" || connectionFor(settings, provider).access === "api"
      ? "api"
      : "cli";
  const key = `${provider}:${transport}`;
  const selected = models?.[key] ?? models?.[provider];
  const catalog = catalogs[provider];
  let model: string;
  if (selected?.mode === "pinned") {
    const entry = catalog?.models.find((item) => item.id === selected.model);
    model = `${entry?.display_name || selected.model}${catalog && !entry ? " (unavailable)" : ""}`;
  } else if (selected?.mode === "role") {
    const role = catalog?.roles.find((item) => item.id === selected.role);
    model = role
      ? `${role.label} (${modelDisplayName(catalog, role.model)})`
      : selected.role;
  } else {
    model = unsetModelSummary(settings, provider, catalog);
  }
  const effort = efforts?.[key] || efforts?.[provider];
  const family =
    providerFamilies.find(({ id }) => id === provider)?.label ?? provider;
  return `${family} · ${model} · ${effort ? capitalize(effort) : unsetEffortLabel(settings, provider)}`;
}

/**
 * Settings whose shared default model does not apply. A review model replaces
 * that default, so an unset choice under it is the CLI's or Pipeline's.
 */
export function withoutDefaultModel(settings: Settings): Settings {
  return { ...settings, default_model: "", default_effort: "" };
}

/** One roster line's model, as in "Claude · Opus 5.5 · Max". */
export function rosterModelSummary(
  settings: Settings,
  entry: RosterEntry,
  catalogs: Record<string, ModelCatalog>,
): string {
  const key = `${entry.provider}:${entry.transport}`;
  return stageModelSummary(
    withoutDefaultModel(settings),
    entry.provider,
    { [key]: entry.model, [entry.provider]: entry.model },
    entry.effort ? { [key]: entry.effort, [entry.provider]: entry.effort } : {},
    catalogs,
  );
}

/** Where a roster line's model comes from, for a short tag. */
export function rosterSourceLabel(entry: RosterEntry): string {
  switch (entry.source) {
    case "review":
      return "This review";
    case "default":
      return "Default model";
    case "workflow":
      return "Workflow";
    default:
      return entry.stage === "fallback" ? "Settings" : "Settings exception";
  }
}

function recordedChooser(provider: string, transport: string): string {
  const family = provider.trim().toLowerCase();
  if (!family) return "Provider";
  if (family === "local") return "Local server";
  return reviewConnectionLabel(family, transport || "cli");
}

/**
 * A recorded call's model. Older reports and CLI calls that did not report a
 * model stored a placeholder; those say who chose instead.
 */
export function recordedModelLabel(
  provider: string,
  transport: string,
  model: string | undefined,
  basis?: string,
): string {
  const value = model?.trim() ?? "";
  if (!value || value === UNRECORDED_MODEL || value.startsWith("Automatic"))
    return `${recordedChooser(provider, transport)}'s choice (not recorded)`;
  return basis === "expected" ? `${value} (expected)` : value;
}

/** A recorded call's thinking level. Empty and "default" name whose default. */
export function recordedEffortLabel(
  provider: string,
  transport: string,
  effort: string | undefined,
): string {
  const value = effort?.trim() ?? "";
  if (!value || value === "default")
    return `${recordedChooser(provider, transport)} default`;
  return value;
}

/** True for a recorded model label that names no model. */
export function isUnrecordedModelLabel(label: string): boolean {
  return label.endsWith("'s choice (not recorded)");
}
