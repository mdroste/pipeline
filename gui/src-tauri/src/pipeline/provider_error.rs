//! Provider-failure classification used by retry and fallback policy.
//!
//! Keep this deliberately narrow. Ordinary request-rate throttling and
//! temporary service overloads already have bounded retries; only durable
//! account/plan exhaustion should switch providers or stop step retries.

/// True when a provider error says the account, subscription window, or API
/// credit/quota has been exhausted. This intentionally does not match a plain
/// "rate limit" or "capacity" message, which is usually transient.
pub fn is_usage_limit_error(error: &str) -> bool {
    let lower = error.to_lowercase();

    // Pipeline's own filesystem quotas are safety failures, never provider
    // entitlement failures.
    if (lower.contains("artifact") || lower.contains("safety quota")) && lower.contains("quota") {
        return false;
    }

    [
        "hit your limit",
        "reached your limit",
        "usage limit reached",
        "usage limit exceeded",
        "usagelimitexceeded",
        "session limit reached",
        "subscription limit",
        "plan limit",
        "weekly limit",
        "5-hour limit",
        "5 hour limit",
        "five-hour limit",
        "five hour limit",
        "quota exceeded",
        "quota_exceeded",
        "insufficient_quota",
        "billing hard limit",
        "credit balance is too low",
        "out of credits",
        "no credits remaining",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

/// Runtime uncertainty and rejected capabilities must never enter a model retry loop.
pub fn is_non_retryable_error(error: &str) -> bool {
    [
        "[codex-outcome-unknown]",
        "[codex-capability]",
        "[codex-auth]",
    ]
    .iter()
    .any(|marker| error.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_plan_exhaustion_from_transient_throttling() {
        assert!(is_usage_limit_error(
            "You've hit your limit · resets 3am (America/Los_Angeles)"
        ));
        assert!(is_usage_limit_error(
            "OpenAI: insufficient_quota: credit balance is too low"
        ));
        assert!(is_usage_limit_error(
            "Google API error: RESOURCE_EXHAUSTED: quota exceeded"
        ));
        assert!(!is_usage_limit_error(
            "Anthropic: Rate limited. Wait a moment and try again."
        ));
        assert!(!is_usage_limit_error(
            "Server is temporarily limiting requests (not your usage limit) · Rate limited"
        ));
        assert!(!is_usage_limit_error(
            "Service overloaded. Try again in a few minutes."
        ));
        assert!(!is_usage_limit_error(
            "Artifact safety quota exceeded (1001 files, 12 MB)"
        ));
    }
}
