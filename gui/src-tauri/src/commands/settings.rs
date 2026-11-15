//! Renderer settings and stored credential handling.

use super::run_queue;

#[derive(serde::Serialize)]
pub struct SettingsResponse {
    pub settings: crate::settings::Settings,
    pub warnings: Vec<String>,
    /// Providers whose stored API key could not be decrypted; the Settings
    /// page offers to discard those so runs can launch again.
    pub undecryptable_keys: Vec<String>,
}

/// Opaque renderer-side marker for a credential that is already stored. The
/// marker carries no secret material; commands resolve it only against the
/// persisted settings and never send it to a provider.
const STORED_SECRET: &str = "__PIPELINE_STORED_SECRET__";

fn redact_settings(mut settings: crate::settings::Settings) -> crate::settings::Settings {
    for secret in [
        &mut settings.anthropic_api_key,
        &mut settings.openai_api_key,
        &mut settings.google_api_key,
        &mut settings.local_api_key,
    ] {
        if !secret.is_empty() {
            *secret = STORED_SECRET.to_string();
        }
    }
    settings
}

/// An exact marker means "keep the saved key". Anything else that contains it
/// is a corrupted key, which would be stored and sent as the credential.
fn refuse_embedded_marker(candidate: &crate::settings::Settings) -> Result<(), String> {
    let corrupted = [
        &candidate.anthropic_api_key,
        &candidate.openai_api_key,
        &candidate.google_api_key,
        &candidate.local_api_key,
    ]
    .into_iter()
    .any(|secret| secret != STORED_SECRET && secret.contains(STORED_SECRET));
    if corrupted {
        return Err(
            "This API key could not be saved because it includes Pipeline's saved-key placeholder. Enter the key again on its own."
                .to_string(),
        );
    }
    Ok(())
}

fn materialize_settings(
    mut candidate: crate::settings::Settings,
    persisted: &crate::settings::Settings,
) -> crate::settings::Settings {
    if candidate.anthropic_api_key == STORED_SECRET {
        candidate
            .anthropic_api_key
            .clone_from(&persisted.anthropic_api_key);
    }
    if candidate.openai_api_key == STORED_SECRET {
        candidate
            .openai_api_key
            .clone_from(&persisted.openai_api_key);
    }
    if candidate.google_api_key == STORED_SECRET {
        candidate
            .google_api_key
            .clone_from(&persisted.google_api_key);
    }
    if candidate.local_api_key == STORED_SECRET {
        // A saved bearer token belongs to one endpoint. Changing the endpoint
        // requires entering its credential again instead of silently forwarding
        // the old token to a new host.
        if candidate.local_base_url == persisted.local_base_url {
            candidate.local_api_key.clone_from(&persisted.local_api_key);
        } else {
            candidate.local_api_key.clear();
        }
    }
    candidate
}

#[tauri::command]
pub async fn get_settings() -> Result<SettingsResponse, String> {
    let (settings, diagnostics) = crate::settings::load_with_diagnostics();
    Ok(SettingsResponse {
        settings: redact_settings(settings),
        warnings: diagnostics.warnings,
        undecryptable_keys: diagnostics.undecryptable_keys,
    })
}

/// Forget a stored API key that no longer decrypts (settings restored without
/// the keyfile). A readable key is cleared from Settings instead.
#[tauri::command]
pub async fn discard_undecryptable_secret(provider: String) -> Result<(), String> {
    crate::settings::discard_undecryptable_secret(&provider)
}

#[tauri::command]
pub async fn save_settings(settings: crate::settings::Settings) -> Result<(), String> {
    refuse_embedded_marker(&settings)?;
    let persisted = crate::settings::load_persisted();
    let settings = materialize_settings(settings, &persisted);
    crate::settings::save_preserving_active(&settings)?;
    // A raised review concurrency limit starts waiting reviews now.
    run_queue::settings_changed();
    Ok(())
}

/// Discover models for the provider's currently active transport. Settings are
/// accepted from the unsaved Settings screen so entering/removing an API key
/// immediately switches between CLI and API catalogs.
#[tauri::command]
pub async fn get_model_catalog(
    provider: String,
    settings: Option<crate::settings::Settings>,
    refresh: Option<bool>,
) -> Result<crate::model_catalog::ModelCatalog, String> {
    if let Some(candidate) = &settings {
        refuse_embedded_marker(candidate)?;
    }
    let persisted = crate::settings::load_persisted();
    let settings = settings
        .map(|candidate| materialize_settings(candidate, &persisted))
        .unwrap_or(persisted)
        .normalized();
    crate::model_catalog::discover(&provider, &settings, refresh.unwrap_or(false)).await
}

#[cfg(test)]
mod settings_response_tests {
    use super::*;

    #[test]
    fn renderer_settings_never_contain_saved_credentials() {
        let settings = crate::settings::Settings {
            anthropic_api_key: "anthropic-secret".into(),
            openai_api_key: "openai-secret".into(),
            google_api_key: "google-secret".into(),
            local_api_key: "local-secret".into(),
            ..Default::default()
        };
        let redacted = redact_settings(settings);
        for secret in [
            redacted.anthropic_api_key,
            redacted.openai_api_key,
            redacted.google_api_key,
            redacted.local_api_key,
        ] {
            assert_eq!(secret, STORED_SECRET);
        }
    }

    #[test]
    fn a_key_pasted_after_the_marker_is_refused_on_every_provider() {
        let pasted = format!("{STORED_SECRET}sk-new-key");
        let candidates = [
            crate::settings::Settings {
                anthropic_api_key: pasted.clone(),
                ..Default::default()
            },
            crate::settings::Settings {
                openai_api_key: pasted.clone(),
                ..Default::default()
            },
            crate::settings::Settings {
                google_api_key: pasted.clone(),
                ..Default::default()
            },
            crate::settings::Settings {
                local_api_key: pasted,
                ..Default::default()
            },
        ];
        for candidate in candidates {
            assert!(refuse_embedded_marker(&candidate).is_err());
        }
    }

    #[test]
    fn the_exact_marker_and_new_keys_are_accepted() {
        let candidate = crate::settings::Settings {
            anthropic_api_key: STORED_SECRET.into(),
            openai_api_key: "sk-new-key".into(),
            ..Default::default()
        };
        assert!(refuse_embedded_marker(&candidate).is_ok());
    }

    #[test]
    fn saved_local_secret_is_not_forwarded_to_a_changed_endpoint() {
        let persisted = crate::settings::Settings {
            local_base_url: "https://trusted.example/v1".into(),
            local_api_key: "trusted-secret".into(),
            ..Default::default()
        };
        let candidate = crate::settings::Settings {
            local_base_url: "https://other.example/v1".into(),
            local_api_key: STORED_SECRET.into(),
            ..persisted.clone()
        };
        assert!(materialize_settings(candidate, &persisted)
            .local_api_key
            .is_empty());
    }
}
