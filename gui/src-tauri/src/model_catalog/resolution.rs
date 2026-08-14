use super::*;

pub async fn discover(
    provider: &str,
    settings: &Settings,
    refresh: bool,
) -> Result<ModelCatalog, String> {
    // A parallel wave can reach resolution simultaneously. Serialize the
    // initial refresh so only one CLI/API probe runs; followers consume the
    // cache written by the leader.
    let _guard = DISCOVERY_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await;
    let provider = if provider.is_empty() {
        "claude"
    } else {
        provider
    };
    let transport = settings.model_transport(provider);
    // API credentials have a stable, non-secret fingerprint. CLI account
    // identity does not, so never reuse an installed-CLI catalog across calls:
    // the user may have switched accounts since it was written.
    let cacheable = provider != "local" && transport == "api";
    if !refresh && cacheable {
        if let Some(cache) = read_cache(provider, transport, settings) {
            // Builds before Claude Agent SDK discovery cached only the three
            // policy roles and an empty model list. Refresh those legacy
            // envelopes immediately after upgrading instead of preserving
            // them for the remainder of the 24-hour TTL.
            let legacy_claude_cli_cache = provider == "claude"
                && transport == "cli"
                && cache.catalog.source == "installed_cli"
                && cache.catalog.models.is_empty();
            if !legacy_claude_cli_cache
                && now_epoch().saturating_sub(cache.saved_at) < CACHE_TTL.as_secs()
            {
                return Ok(cache.catalog);
            }
        }
    }
    let policy = bundled_policy();
    let live = if transport == "cli" {
        cli_catalog(provider).await
    } else {
        api_catalog(provider, settings).await
    };
    match live {
        Ok(mut catalog) => {
            apply_policy(&mut catalog, &policy);
            let envelope = CacheEnvelope {
                schema_version: CACHE_SCHEMA_VERSION,
                policy_fingerprint: bundled_policy_fingerprint(),
                credential_fingerprint: catalog_credential_fingerprint(
                    provider, transport, settings,
                ),
                saved_at: now_epoch(),
                catalog: catalog.clone(),
            };
            if cacheable {
                let _ = write_json_atomic(&cache_path(provider, transport)?, &envelope);
            }
            Ok(catalog)
        }
        Err(error) => {
            if cacheable {
                if let Some(mut cache) = read_cache(provider, transport, settings) {
                    cache.catalog.stale = true;
                    cache.catalog.warning = Some(format!(
                        "Live discovery failed; using the last known catalog. {error}"
                    ));
                    return Ok(cache.catalog);
                }
            }
            let mut catalog = base_catalog(provider, transport, "bundled_policy");
            catalog.stale = true;
            catalog.warning = Some(format!(
                "Live discovery failed. Automatic still uses the provider default. {error}"
            ));
            if let Some(transport_policy) = policy_for(&policy, provider, transport) {
                catalog.models = transport_policy
                    .models
                    .iter()
                    .map(|model| {
                        let mut entry = entry(&model.id);
                        entry.deprecated = model.deprecated;
                        entry.replacement.clone_from(&model.replacement);
                        entry.input_price_per_million = model.input_price_per_million;
                        entry.output_price_per_million = model.output_price_per_million;
                        entry
                    })
                    .collect();
            }
            apply_policy(&mut catalog, &policy);
            Ok(catalog)
        }
    }
}

pub async fn resolve(
    provider: &str,
    settings: &Settings,
    step_selection: Option<&ModelSelection>,
) -> Result<ResolvedModel, String> {
    let provider = if provider.is_empty() {
        "claude"
    } else {
        provider
    };
    let transport = settings.model_transport(provider).to_string();
    if transport == "api" {
        let key_present = match provider {
            "claude" => !settings.anthropic_api_key.trim().is_empty(),
            "codex" => !settings.openai_api_key.trim().is_empty(),
            "antigravity" => !settings.google_api_key.trim().is_empty(),
            "local" => true,
            _ => false,
        };
        if !key_present {
            return Err(format!(
                "{provider} API mode is selected, but its API key is missing. Add the key in Settings → API Keys or switch to Subscription mode."
            ));
        }
    }
    let selection = step_selection
        .cloned()
        .unwrap_or_else(|| settings.model_selection(provider));

    // CLI Automatic is deliberately zero-probe at run time: omitting the flag
    // lets the installed CLI choose whatever that installation supports.
    if transport == "cli" && selection == ModelSelection::Automatic {
        return Ok(ResolvedModel {
            selection,
            command_model: None,
            resolved_model: "Provider default".to_string(),
            transport,
            source: "provider_default".to_string(),
            catalog_updated_at: String::new(),
            supported_efforts: Vec::new(),
        });
    }

    let catalog = discover(provider, settings, false).await?;
    let command_model = match &selection {
        ModelSelection::Automatic => catalog
            .recommended_model
            .clone()
            .or_else(|| catalog.default_model.clone())
            .or_else(|| catalog.models.first().map(|m| m.id.clone())),
        ModelSelection::Role { role } => {
            let normalized = match (provider, role.as_str()) {
                ("claude", "sonnet") => "balanced",
                ("claude", "opus" | "fable") => "quality",
                ("claude", "haiku") => "fast",
                _ => role,
            };
            catalog
                .roles
                .iter()
                .find(|r| r.id == normalized || r.model == *role)
                .map(|r| r.model.clone())
        }
        ModelSelection::Pinned { model } => {
            if !catalog.stale
                && !catalog.models.is_empty()
                && !catalog.models.iter().any(|m| m.id == *model)
            {
                return Err(format!("Pinned model '{model}' is not available for {provider} {}. Choose another model or Automatic.", catalog.transport));
            }
            Some(model.clone())
        }
    };
    let command_model = command_model.ok_or_else(|| {
        format!(
            "Could not resolve {} for {provider} {}",
            selection.label(),
            catalog.transport
        )
    })?;
    let supported_efforts = catalog
        .models
        .iter()
        .find(|m| m.id == command_model)
        .map(|m| m.supported_efforts.clone())
        .unwrap_or_default();
    Ok(ResolvedModel {
        selection,
        resolved_model: command_model.clone(),
        command_model: Some(command_model),
        transport: catalog.transport,
        source: catalog.source,
        catalog_updated_at: catalog.fetched_at,
        supported_efforts,
    })
}

/// Pricing remains available synchronously to report rendering and is tied to
/// the policy embedded in the signed application build.
pub fn price_for_model(model: &str) -> Option<(f64, f64)> {
    let model = model.to_ascii_lowercase();
    let policy = bundled_policy();
    policy
        .transports
        .values()
        .flat_map(|p| p.models.iter())
        .filter(|m| {
            model == m.id.to_ascii_lowercase() || model.contains(&m.id.to_ascii_lowercase())
        })
        // A full model id can contain another valid id (for example,
        // `gpt-4.1-mini` contains `gpt-4.1`). Prefer the most specific match
        // instead of depending on policy-file ordering.
        .filter_map(|m| {
            Some((
                m.id.len(),
                m.input_price_per_million?,
                m.output_price_per_million?,
            ))
        })
        .max_by_key(|(specificity, _, _)| *specificity)
        .map(|(_, input, output)| (input, output))
}
