import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

import type { ModelCatalog, Settings } from "../../lib/types";
import { providerTransport, PROVIDERS } from "../../lib/providers";

function catalogDiscoveryInputs(settings: Settings): Record<string, string> {
  return {
    claude: `${settings.claude_access_mode}\u0000${settings.anthropic_api_key}`,
    codex: `${settings.codex_access_mode}\u0000${settings.codex_backend ?? "app_server"}\u0000${settings.openai_api_key}`,
    antigravity: `${settings.antigravity_access_mode}\u0000${settings.google_api_key}`,
    local: `${settings.local_base_url}\u0000${settings.local_api_key}`,
  };
}

export function useProviderCatalogs(settings: Settings | null) {
  const [catalogs, setCatalogs] = useState<Record<string, ModelCatalog>>({});
  const [catalogLoading, setCatalogLoading] = useState<Record<string, boolean>>(
    {},
  );
  const [catalogInputKeys, setCatalogInputKeys] = useState<
    Record<string, string>
  >({});
  const [savedCatalogInputs, setSavedCatalogInputs] = useState<Record<
    string,
    string
  > | null>(null);
  const savedCatalogInputsRef = useRef<Record<string, string> | null>(
    savedCatalogInputs,
  );
  const catalogRequestsRef = useRef<Record<string, number>>({});
  const previousDraftDiscoveryInputsRef = useRef<Record<string, string> | null>(
    null,
  );
  const initialCatalogDiscoveryStartedRef = useRef(false);
  savedCatalogInputsRef.current = savedCatalogInputs;
  const loadCatalog = async (
    provider: string,
    current: Settings,
    refresh = false,
  ) => {
    const request = (catalogRequestsRef.current[provider] ?? 0) + 1;
    const inputKey = catalogDiscoveryInputs(current)[provider];
    catalogRequestsRef.current[provider] = request;
    setCatalogLoading((old) => ({ ...old, [provider]: true }));
    try {
      const catalog = await invoke<ModelCatalog>("get_model_catalog", {
        provider,
        settings: current,
        refresh,
      });
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogs((old) => ({ ...old, [provider]: catalog }));
        setCatalogInputKeys((old) => ({ ...old, [provider]: inputKey }));
      }
    } catch (error) {
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogs((old) => ({
          ...old,
          [provider]: {
            provider,
            transport:
              provider === "local" ||
              providerTransport(current, provider) === "api"
                ? "api"
                : "cli",
            source: "unavailable",
            source_version: "",
            fetched_at: "",
            stale: true,
            warning: String(error),
            models: [],
            roles: [],
          },
        }));
        setCatalogInputKeys((old) => ({ ...old, [provider]: inputKey }));
      }
    } finally {
      if (catalogRequestsRef.current[provider] === request) {
        setCatalogLoading((old) => ({ ...old, [provider]: false }));
      }
    }
  };

  // Load catalogs once from settings that are already persisted. Edited
  // credentials trigger authenticated discovery only after autosave succeeds.
  useEffect(() => {
    if (
      !settings ||
      !savedCatalogInputs ||
      initialCatalogDiscoveryStartedRef.current
    )
      return;
    initialCatalogDiscoveryStartedRef.current = true;
    for (const provider of PROVIDERS) {
      void loadCatalog(provider, settings);
    }
  }, [savedCatalogInputs, settings]);

  // Invalidate both visible data and in-flight requests as soon as a discovery
  // input changes. Autosave or an explicit Refresh must complete before models
  // for the draft credential/server become available.
  useEffect(() => {
    if (!settings) return;
    const current = catalogDiscoveryInputs(settings);
    const previous = previousDraftDiscoveryInputsRef.current;
    previousDraftDiscoveryInputsRef.current = current;
    if (!previous) return;
    const changed = PROVIDERS.filter(
      (provider) => previous[provider] !== current[provider],
    );
    if (changed.length === 0) return;
    for (const provider of changed) {
      catalogRequestsRef.current[provider] =
        (catalogRequestsRef.current[provider] ?? 0) + 1;
    }
    setCatalogs((old) => {
      const next = { ...old };
      changed.forEach((provider) => delete next[provider]);
      return next;
    });
    setCatalogInputKeys((old) => {
      const next = { ...old };
      changed.forEach((provider) => delete next[provider]);
      return next;
    });
    setCatalogLoading((old) => ({
      ...old,
      ...Object.fromEntries(changed.map((provider) => [provider, false])),
    }));
  }, [
    settings?.claude_access_mode,
    settings?.codex_access_mode,
    settings?.codex_backend,
    settings?.antigravity_access_mode,
    settings?.anthropic_api_key,
    settings?.openai_api_key,
    settings?.google_api_key,
    settings?.local_base_url,
    settings?.local_api_key,
  ]);

  const initialize = (current: Settings) => {
    const inputs = catalogDiscoveryInputs(current);
    previousDraftDiscoveryInputsRef.current = inputs;
    setSavedCatalogInputs(inputs);
  };
  const didSave = (settingsToSave: Settings) => {
    const nextCatalogInputs = catalogDiscoveryInputs(settingsToSave);
    const previousCatalogInputs =
      savedCatalogInputsRef.current ?? nextCatalogInputs;
    const changedProviders = PROVIDERS.filter(
      (provider) =>
        previousCatalogInputs[provider] !== nextCatalogInputs[provider],
    );
    savedCatalogInputsRef.current = nextCatalogInputs;
    setSavedCatalogInputs(nextCatalogInputs);
    for (const provider of changedProviders) {
      // Cloud catalog caches do not include account identity, so a
      // changed saved credential must bypass them. Local discovery is
      // uncached.
      void loadCatalog(provider, settingsToSave, provider !== "local");
    }
  };
  const draftCatalogInputs = settings ? catalogDiscoveryInputs(settings) : {};
  const catalogBlocked = Object.fromEntries(
    PROVIDERS.map((provider) => [
      provider,
      catalogInputKeys[provider] !== draftCatalogInputs[provider],
    ]),
  ) as Record<string, boolean>;
  const discoveryInputChanged = Object.fromEntries(
    PROVIDERS.map((provider) => [
      provider,
      savedCatalogInputs?.[provider] !== draftCatalogInputs[provider],
    ]),
  ) as Record<string, boolean>;

  return {
    initialize,
    didSave,
    catalogs,
    catalogLoading,
    catalogBlocked,
    discoveryInputChanged,
    loadCatalog,
  };
}
