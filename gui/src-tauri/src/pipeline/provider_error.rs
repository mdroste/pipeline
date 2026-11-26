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
        "exceeded your current quota",
        "insufficient_quota",
        "billing hard limit",
        "credit balance is too low",
        "out of credits",
        "no credits remaining",
    ]
    .iter()
    .any(|pattern| lower.contains(pattern))
}

/// True when a provider refused a call because requests arrived too quickly.
/// Exhausted plans are excluded: waiting a moment cannot restore them.
pub fn is_throttling_error(error: &str) -> bool {
    let lower = error.to_lowercase();
    !is_usage_limit_error(error)
        && ["too many requests", "rate limit", "rate_limit"]
            .iter()
            .any(|pattern| lower.contains(pattern))
}

/// Opens the error of a step stopped for what it wrote to disk.
pub const ARTIFACT_LIMIT_PREFIX: &str = "Stopped at the artifact safety limit:";

/// Runtime uncertainty, rejected capabilities, and a breach of the artifact
/// safety limit must never enter a model retry loop.
pub fn is_non_retryable_error(error: &str) -> bool {
    [
        "[codex-outcome-unknown]",
        "[codex-capability]",
        "[codex-auth]",
        "[claude-auth]",
        ARTIFACT_LIMIT_PREFIX,
        super::executor::budget::BUDGET_STOP_PREFIX,
    ]
    .iter()
    .any(|marker| error.contains(marker))
}

/// Compatibility classification at the provider-string boundary. Display text
/// stays untouched; policies use named facts instead of repeating marker scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureKind {
    Cancelled,
    OutcomeUnknown,
    Capability,
    Authentication,
    ArtifactLimit,
    BudgetLimit,
    UsageExhausted,
    Throttled,
    Other,
}
#[derive(Debug)]
pub(crate) struct Failure<'a> {
    pub message: &'a str,
    pub kind: FailureKind,
    usage_exhausted: bool,
    non_retryable: bool,
    throttled: bool,
}
impl<'a> Failure<'a> {
    pub fn from_legacy(message: &'a str) -> Self {
        let usage_exhausted = is_usage_limit_error(message);
        let non_retryable = is_non_retryable_error(message);
        let throttled = is_throttling_error(message);
        let kind = if crate::cancellation::is_pipeline_cancellation_error(message) {
            FailureKind::Cancelled
        } else if message.contains("[codex-outcome-unknown]") {
            FailureKind::OutcomeUnknown
        } else if message.contains("[codex-capability]") {
            FailureKind::Capability
        } else if message.contains("[codex-auth]") || message.contains("[claude-auth]") {
            FailureKind::Authentication
        } else if message.contains(ARTIFACT_LIMIT_PREFIX) {
            FailureKind::ArtifactLimit
        } else if message.contains(super::executor::budget::BUDGET_STOP_PREFIX) {
            FailureKind::BudgetLimit
        } else if usage_exhausted {
            FailureKind::UsageExhausted
        } else if throttled {
            FailureKind::Throttled
        } else {
            FailureKind::Other
        };
        Self {
            message,
            kind,
            usage_exhausted,
            non_retryable,
            throttled,
        }
    }
    pub fn permits_fallback(&self) -> bool {
        self.usage_exhausted && !self.non_retryable
    }
    pub fn stops_retries(&self) -> bool {
        self.usage_exhausted || self.non_retryable
    }
    pub fn is_throttled(&self) -> bool {
        self.throttled
    }
    pub fn is_cancelled(&self) -> bool {
        self.kind == FailureKind::Cancelled
    }
}
impl std::fmt::Display for Failure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_throttling_but_not_plan_exhaustion_or_other_failures() {
        assert!(is_throttling_error(
            "Codex call failed (exit 1): exceeded retry limit, last status: 429 Too Many Requests"
        ));
        assert!(is_throttling_error(
            "Anthropic: Rate limited. Wait a moment and try again."
        ));
        assert!(!is_throttling_error(
            "Rate limit reached: exceeded your current quota"
        ));
        assert!(!is_throttling_error(
            "Service overloaded. Try again in a few minutes."
        ));
        assert!(!is_throttling_error(
            "OAuth refresh token was rejected: invalid_grant"
        ));
    }

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

#[cfg(test)]
mod typed_tests {
    use super::{Failure, FailureKind};
    #[test]
    fn uncertainty_and_safety_prevent_fallback_even_with_quota_words() {
        let message = "[codex-outcome-unknown] quota exceeded";
        let failure = Failure::from_legacy(message);
        assert_eq!(failure.kind, FailureKind::OutcomeUnknown);
        assert!(failure.stops_retries());
        assert!(!failure.permits_fallback());
        assert_eq!(failure.to_string(), message);
        let quota = Failure::from_legacy("insufficient_quota");
        assert!(quota.permits_fallback());
        assert!(quota.stops_retries());
        assert!(!Failure::from_legacy("subscription cancelled").is_cancelled());
    }
}
