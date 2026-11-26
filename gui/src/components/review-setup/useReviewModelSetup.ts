import { useEffect, useMemo, useState } from "react";
import { useReviewSetupEnvironment } from "../reviewSetupEnvironment";
import { PROVIDERS, PROVIDER_LABELS, type Provider } from "../../lib/providers";
import type { Settings } from "../../lib/settingsTypes";
import type { PipelineConfig } from "../../lib/workflowTypes";
import type {
  ModelCatalog,
  RosterEntry,
  RunModelOverrides,
} from "../../lib/reviewModelTypes";
import { useSetupReads } from "../reviewSetupReads";
import {
  needsReviewTeam,
  primaryReview,
  withoutCombiningOverride,
} from "../../lib/reviewTeam";

export function useReviewModelSetup(
  value: RunModelOverrides | null,
  refreshKey: number,
  localLlmActive: boolean,
) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [config, setConfig] = useState<PipelineConfig | null>(null);
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [catalogErrors, setCatalogErrors] = useState<string[]>([]);
  const [roster, setRoster] = useState<RosterEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [rosterError, setRosterError] = useState<string | null>(null);
  const [rosterLoading, setRosterLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  const reads = useSetupReads(refreshKey);
  const environment = useReviewSetupEnvironment();
  useEffect(() => {
    let live = true;
    setConfig(null);
    setLoadError(null);
    Promise.all([environment.loadSettings(), reads.config()])
      .then(([response, workflow]) => {
        if (live) {
          setSettings(response.settings);
          setConfig(workflow);
        }
      })
      .catch((error) => {
        if (live) setLoadError(String(error));
      });
    return () => {
      live = false;
    };
  }, [reads, attempt, environment]);
  const providers = useMemo(
    () =>
      PROVIDERS.filter(
        (provider) =>
          provider !== "local" ||
          (localLlmActive && Boolean(settings?.local_model.trim())),
      ) as Provider[],
    [settings, localLlmActive],
  );
  useEffect(() => {
    if (!settings) return;
    let live = true;
    setCatalogs({});
    setCatalogErrors([]);
    Promise.allSettled(
      providers.map(
        async (provider) =>
          [
            provider,
            await environment.modelCatalog({
              provider,
              settings,
              refresh: false,
            }),
          ] as const,
      ),
    ).then((results) => {
      if (!live) return;
      setCatalogErrors(
        results.flatMap((result, index) =>
          result.status === "rejected"
            ? [PROVIDER_LABELS[providers[index]]]
            : [],
        ),
      );
      setCatalogs(
        Object.fromEntries(
          results.flatMap((result) =>
            result.status === "fulfilled" ? [result.value] : [],
          ),
        ),
      );
    });
    return () => {
      live = false;
    };
  }, [settings, providers, attempt, environment]);
  useEffect(() => {
    if (!settings || !config) return;
    let live = true;
    setRosterLoading(true);
    setRosterError(null);
    setRoster(null);
    environment
      .modelRoster({
        runParallelOverrides: needsReviewTeam(value)
          ? primaryReview(settings, value)
          : withoutCombiningOverride(value),
      })
      .then((entries) => {
        if (live) {
          setRoster(entries);
          setRosterLoading(false);
        }
      })
      .catch((error) => {
        if (live) {
          setRosterError(String(error));
          setRosterLoading(false);
        }
      });
    return () => {
      live = false;
    };
  }, [settings, config, value, refreshKey, attempt, environment]);
  return {
    settings,
    config,
    catalogs,
    providers,
    catalogErrors,
    roster,
    loadError,
    rosterError,
    rosterLoading,
    retry: () => setAttempt((n) => n + 1),
  };
}
