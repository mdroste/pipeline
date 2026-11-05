//! Provider transport, inherited role policies, and compatibility normalization.

use super::types::{
    default_codex_backend, default_paddle_full_layout_threshold, default_provider, ModelSelection,
    Settings,
};

const FORMER_PADDLE_FULL_LAYOUT_THRESHOLD: f32 = 0.5;

impl ModelSelection {
    pub fn from_legacy(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty() {
            return Self::Automatic;
        }
        match value {
            "auto" | "automatic" => Self::Automatic,
            "sonnet" | "opus" | "haiku" | "fable" => Self::Role {
                role: value.to_string(),
            },
            _ => Self::Pinned {
                model: value.to_string(),
            },
        }
    }

    pub fn legacy_value(&self) -> String {
        match self {
            Self::Automatic => String::new(),
            Self::Role { role } => role.clone(),
            Self::Pinned { model } => model.clone(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Automatic => "Automatic".to_string(),
            Self::Role { role } => format!("{role} role"),
            Self::Pinned { model } => model.clone(),
        }
    }
}

impl Settings {
    /// "cli" for subscription-backed command-line providers and "api" for
    /// direct HTTP/local providers.
    pub fn model_transport(&self, provider: &str) -> &'static str {
        crate::connections::ConnectionId::for_review(provider, self)
            .map(|connection| connection.legacy_transport())
            .unwrap_or("cli")
    }

    pub fn model_context_key(&self, provider: &str) -> String {
        format!(
            "{}:{}",
            if provider.is_empty() {
                "claude"
            } else {
                provider
            },
            self.model_transport(provider)
        )
    }

    pub fn model_selection(&self, provider: &str) -> ModelSelection {
        match provider {
            "local" => {
                if self.local_model.trim().is_empty() {
                    ModelSelection::Automatic
                } else {
                    ModelSelection::Pinned {
                        model: self.local_model.trim().to_string(),
                    }
                }
            }
            // The shared default fills in for a missing role or workflow
            // override on its own provider. Other providers delegate to the
            // active provider/transport.
            _ if provider == self.preferred_provider && !self.default_model.is_empty() => {
                ModelSelection::Pinned {
                    model: self.default_model.clone(),
                }
            }
            _ => ModelSelection::Automatic,
        }
    }

    pub fn model_effort(&self, provider: &str) -> &str {
        // As with model selection, the shared default applies to its own
        // provider. The legacy provider effort fields remain in the wire
        // format only so older settings and bundles still deserialize.
        if provider == self.preferred_provider {
            &self.default_effort
        } else {
            ""
        }
    }

    pub fn parallel_agents(&self) -> Vec<String> {
        if self.default_parallel_agents.is_empty() {
            vec![self.preferred_provider.clone()]
        } else {
            self.default_parallel_agents.clone()
        }
    }

    pub fn sequential_agent(&self) -> &str {
        if self.default_sequential_agent.trim().is_empty() {
            &self.preferred_provider
        } else {
            &self.default_sequential_agent
        }
    }

    pub fn merge_agent(&self) -> &str {
        if self.default_merge_agent.trim().is_empty() {
            self.sequential_agent()
        } else {
            &self.default_merge_agent
        }
    }

    pub fn orientation_agent(&self) -> &str {
        if self.default_orientation_agent.trim().is_empty() {
            &self.preferred_provider
        } else {
            &self.default_orientation_agent
        }
    }

    fn role_model_selection(
        &self,
        overrides: &std::collections::HashMap<String, ModelSelection>,
        provider: &str,
    ) -> Option<ModelSelection> {
        overrides
            .get(&self.model_context_key(provider))
            .or_else(|| overrides.get(provider))
            .cloned()
    }

    /// The stage's own thinking level, without the shared default.
    fn stage_effort(
        &self,
        overrides: &std::collections::HashMap<String, String>,
        provider: &str,
    ) -> String {
        overrides
            .get(&self.model_context_key(provider))
            .or_else(|| overrides.get(provider))
            .filter(|effort| !effort.trim().is_empty())
            .cloned()
            .unwrap_or_default()
    }

    fn role_effort(
        &self,
        overrides: &std::collections::HashMap<String, String>,
        provider: &str,
    ) -> String {
        let effort = self.stage_effort(overrides, provider);
        if effort.is_empty() {
            self.model_effort(provider).to_string()
        } else {
            effort
        }
    }

    pub fn parallel_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_parallel_model_overrides, provider)
    }

    pub fn parallel_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_parallel_effort_overrides, provider)
    }

    pub fn sequential_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_sequential_model_overrides, provider)
    }

    pub fn merge_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_merge_model_overrides, provider)
            .or_else(|| {
                self.default_merge_agent
                    .trim()
                    .is_empty()
                    .then(|| self.sequential_model_selection(provider))
                    .flatten()
            })
    }

    pub fn merge_effort(&self, provider: &str) -> String {
        let configured = self.stage_effort(&self.default_merge_effort_overrides, provider);
        if !configured.is_empty() {
            configured
        } else if self.default_merge_agent.trim().is_empty() {
            self.sequential_effort(provider)
        } else {
            self.model_effort(provider).to_string()
        }
    }

    pub fn sequential_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_sequential_effort_overrides, provider)
    }

    pub fn orientation_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.default_orientation_model_overrides, provider)
    }

    pub fn orientation_effort(&self, provider: &str) -> String {
        self.role_effort(&self.default_orientation_effort_overrides, provider)
    }

    pub fn usage_limit_fallback_agent(&self) -> Option<&str> {
        let provider = self.usage_limit_fallback_agent.trim();
        (!provider.is_empty()).then_some(provider)
    }

    pub fn usage_limit_fallback_model_selection(&self, provider: &str) -> Option<ModelSelection> {
        self.role_model_selection(&self.usage_limit_fallback_model_overrides, provider)
    }

    pub fn usage_limit_fallback_effort(&self, provider: &str) -> String {
        self.role_effort(&self.usage_limit_fallback_effort_overrides, provider)
    }

    /// Preserve old model fields when settings are opened and re-saved. These
    /// selections are no longer provider-level runtime defaults.
    pub(super) fn migrate_legacy_model_fields(&mut self) {
        if self.claude_cli_model_selection == ModelSelection::Automatic
            && self.claude_api_model_selection == ModelSelection::Automatic
            && !self.claude_model.trim().is_empty()
        {
            let migrated = ModelSelection::from_legacy(&self.claude_model);
            self.claude_cli_model_selection = migrated.clone();
            self.claude_api_model_selection = migrated;
        }
        if self.codex_cli_model_selection == ModelSelection::Automatic
            && self.codex_api_model_selection == ModelSelection::Automatic
            && !self.codex_model.trim().is_empty()
        {
            let migrated = ModelSelection::from_legacy(&self.codex_model);
            self.codex_cli_model_selection = migrated.clone();
            self.codex_api_model_selection = migrated;
        }
    }

    /// Keep old Pipeline builds and exported settings usable while clearing the
    /// retired provider-level default from newly saved settings.
    pub(super) fn sync_legacy_model_fields(&mut self) {
        self.claude_model = ModelSelection::Automatic.legacy_value();
        self.codex_model = ModelSelection::Automatic.legacy_value();
    }

    pub fn normalized(mut self) -> Self {
        self.migrate_legacy_model_fields();
        self.normalize_access_modes();
        self.drop_unknown_providers();
        self
    }

    /// Migrate the pre-mode behavior exactly once. An absent mode follows the
    /// old rule (stored key => API, otherwise subscription); an explicit mode
    /// is never changed merely because a credential is entered or cleared.
    /// Antigravity is the exception: subscription mode is disabled, so any
    /// stored value is coerced to "api" and older settings files keep loading.
    /// Codex's native backend migrates independently of subscription/API mode,
    /// and so does the former PaddleOCR-VL layout threshold default.
    pub(super) fn normalize_access_modes(&mut self) {
        if self.codex_backend_preference_version == 0 {
            if self.codex_backend == "legacy_cli" {
                self.codex_backend = default_codex_backend();
            }
            self.codex_backend_preference_version = 1;
        }
        if self.paddle_full_layout_threshold_version == 0 {
            if self.paddle_full_layout_threshold == FORMER_PADDLE_FULL_LAYOUT_THRESHOLD {
                self.paddle_full_layout_threshold = default_paddle_full_layout_threshold();
            }
            self.paddle_full_layout_threshold_version = 1;
        }
        if self.model_defaults_version == 0 {
            self.release_stage_defaults();
            self.model_defaults_version = 1;
        }
        if self.claude_access_mode.trim().is_empty() {
            self.claude_access_mode = if self.anthropic_api_key.trim().is_empty() {
                "subscription"
            } else {
                "api"
            }
            .to_string();
        }
        if self.codex_access_mode.trim().is_empty() {
            self.codex_access_mode = if self.openai_api_key.trim().is_empty() {
                "subscription"
            } else {
                "api"
            }
            .to_string();
        }
        self.antigravity_access_mode = "api".to_string();
    }

    /// Stages used to be saved with a copy of the preferred provider, which
    /// left them behind whenever it changed. Empty now means "follow the
    /// default", so clear the copies that resolve to the same thing. A stage
    /// with its own model or thinking is a deliberate choice and stays.
    fn release_stage_defaults(&mut self) {
        let parallel_plain = self.default_parallel_model_overrides.is_empty()
            && self.default_parallel_effort_overrides.is_empty();
        let sequential_plain = self.default_sequential_model_overrides.is_empty()
            && self.default_sequential_effort_overrides.is_empty();
        let merge_plain = self.default_merge_model_overrides.is_empty()
            && self.default_merge_effort_overrides.is_empty();
        let orientation_plain = self.default_orientation_model_overrides.is_empty()
            && self.default_orientation_effort_overrides.is_empty();
        // Merge inherits from Sequential, including its model and thinking.
        if merge_plain && sequential_plain && self.default_merge_agent == self.sequential_agent() {
            self.default_merge_agent.clear();
        }
        if sequential_plain && self.default_sequential_agent == self.preferred_provider {
            self.default_sequential_agent.clear();
        }
        if orientation_plain && self.default_orientation_agent == self.preferred_provider {
            self.default_orientation_agent.clear();
        }
        if parallel_plain && self.default_parallel_agents == [self.preferred_provider.clone()] {
            self.default_parallel_agents.clear();
        }
    }

    /// Drop provider ids this build does not support from the surviving
    /// agent-default fields and override maps. Settings written by earlier
    /// builds can still name removed providers (for example "gemini");
    /// validation on the run-launch path rejects them, and the Settings UI
    /// can neither display nor delete them, so they are normalized away at
    /// load instead.
    pub(super) fn drop_unknown_providers(&mut self) {
        const KNOWN_PROVIDERS: [&str; 4] = ["claude", "codex", "antigravity", "local"];
        let known = |provider: &str| KNOWN_PROVIDERS.contains(&provider);
        if !known(&self.preferred_provider) {
            self.preferred_provider = default_provider();
        }
        self.default_parallel_agents
            .retain(|provider| known(provider));
        if !self.default_sequential_agent.is_empty() && !known(&self.default_sequential_agent) {
            self.default_sequential_agent = String::new();
        }
        if !self.default_merge_agent.is_empty() && !known(&self.default_merge_agent) {
            self.default_merge_agent = String::new();
        }
        if !self.default_orientation_agent.is_empty() && !known(&self.default_orientation_agent) {
            self.default_orientation_agent = String::new();
        }
        if !self.usage_limit_fallback_agent.is_empty() && !known(&self.usage_limit_fallback_agent) {
            self.usage_limit_fallback_agent = String::new();
        }
        let known_key = |key: &str| {
            KNOWN_PROVIDERS.iter().any(|provider| {
                key == *provider
                    || key == format!("{provider}:cli")
                    || key == format!("{provider}:api")
            })
        };
        for overrides in [
            &mut self.default_parallel_model_overrides,
            &mut self.default_merge_model_overrides,
            &mut self.default_sequential_model_overrides,
            &mut self.default_orientation_model_overrides,
            &mut self.usage_limit_fallback_model_overrides,
        ] {
            overrides.retain(|key, _| known_key(key));
        }
        for overrides in [
            &mut self.default_parallel_effort_overrides,
            &mut self.default_merge_effort_overrides,
            &mut self.default_sequential_effort_overrides,
            &mut self.default_orientation_effort_overrides,
            &mut self.usage_limit_fallback_effort_overrides,
        ] {
            overrides.retain(|key, _| known_key(key));
        }
    }
}
