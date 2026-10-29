//! When automatic update checks run and how failures delay the next one.

use chrono::{DateTime, Duration, Utc};
use std::sync::OnceLock;

use super::github::{CheckError, Release};
use super::state::State;

/// Lets store recovery and startup I/O finish before the first request.
const STARTUP_DELAY: std::time::Duration = std::time::Duration::from_secs(60);
/// Wall-clock comparisons on each tick keep the schedule correct across sleep.
const TICK: std::time::Duration = std::time::Duration::from_secs(60 * 60);
const CHECK_INTERVAL_HOURS: i64 = 24;
const MAX_WAIT_HOURS: i64 = 24;

static STARTED: OnceLock<()> = OnceLock::new();

/// Only builds from the official release workflow check on their own;
/// development and source builds check when the researcher asks.
pub(super) fn automatic_supported() -> bool {
    option_env!("PIPELINE_OFFICIAL_RELEASE") == Some("1")
}

pub fn start(app: tauri::AppHandle) {
    if !automatic_supported() || STARTED.set(()).is_err() {
        return;
    }
    let bus = crate::emit::from_app(app);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            super::run_automatic(&bus).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

pub(super) fn automatic_due(state: &State, now: DateTime<Utc>) -> bool {
    state.automatic
        && state.rate_limited_until.is_none_or(|until| now >= until)
        && state.retry_after.is_none_or(|until| now >= until)
        // A last check in the future means the clock moved back; check again.
        && state.last_checked_at.is_none_or(|at| {
            at > now || now >= at + Duration::hours(CHECK_INTERVAL_HOURS)
        })
}

/// One hour after the first failure, doubling up to a day.
fn backoff(failures: u32) -> Duration {
    let hours = 1_i64 << failures.saturating_sub(1).min(5);
    Duration::hours(hours.min(MAX_WAIT_HOURS))
}

pub(super) fn apply_outcome(
    state: &mut State,
    outcome: &Result<Option<Release>, CheckError>,
    now: DateTime<Utc>,
) {
    match outcome {
        Ok(latest) => {
            state.last_checked_at = Some(now);
            state.latest = latest.clone();
            state.failures = 0;
            state.retry_after = None;
            state.rate_limited_until = None;
        }
        // Nothing reached GitHub, so the next hourly tick may try again.
        Err(CheckError::Network) => {}
        Err(CheckError::RateLimited { until }) => {
            state.failures = state.failures.saturating_add(1);
            let latest = now + Duration::hours(MAX_WAIT_HOURS);
            let earliest = now + Duration::minutes(1);
            let until = until.unwrap_or_else(|| now + backoff(state.failures));
            state.rate_limited_until = Some(until.clamp(earliest, latest));
        }
        Err(CheckError::Unexpected(_)) => {
            state.failures = state.failures.saturating_add(1);
            state.retry_after = Some(now + backoff(state.failures));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap()
    }

    fn checked(hours_ago: i64) -> State {
        State {
            last_checked_at: Some(now() - Duration::hours(hours_ago)),
            ..State::default()
        }
    }

    #[test]
    fn checks_once_a_day() {
        assert!(automatic_due(&State::default(), now()));
        assert!(!automatic_due(&checked(1), now()));
        assert!(!automatic_due(&checked(23), now()));
        assert!(automatic_due(&checked(24), now()));
        assert!(automatic_due(&checked(-2), now()), "clock moved back");
    }

    #[test]
    fn preference_and_waits_block_automatic_checks() {
        let off = State {
            automatic: false,
            ..State::default()
        };
        assert!(!automatic_due(&off, now()));
        let limited = State {
            rate_limited_until: Some(now() + Duration::minutes(5)),
            ..State::default()
        };
        assert!(!automatic_due(&limited, now()));
        assert!(automatic_due(&limited, now() + Duration::minutes(5)));
        let retrying = State {
            retry_after: Some(now() + Duration::hours(2)),
            ..State::default()
        };
        assert!(!automatic_due(&retrying, now()));
    }

    #[test]
    fn success_records_the_release_and_clears_failures() {
        let mut state = State {
            failures: 3,
            retry_after: Some(now()),
            rate_limited_until: Some(now()),
            ..State::default()
        };
        apply_outcome(&mut state, &Ok(None), now());
        assert_eq!(state.last_checked_at, Some(now()));
        assert_eq!(state.failures, 0);
        assert_eq!(state.retry_after, None);
        assert_eq!(state.rate_limited_until, None);
    }

    #[test]
    fn network_failures_do_not_delay_the_next_tick() {
        let mut state = checked(30);
        apply_outcome(&mut state, &Err(CheckError::Network), now());
        assert_eq!(state, checked(30));
    }

    #[test]
    fn rate_limits_use_github_headers_within_bounds() {
        let mut state = State::default();
        let until = now() + Duration::minutes(40);
        apply_outcome(
            &mut state,
            &Err(CheckError::RateLimited { until: Some(until) }),
            now(),
        );
        assert_eq!(state.rate_limited_until, Some(until));

        apply_outcome(
            &mut state,
            &Err(CheckError::RateLimited {
                until: Some(now() + Duration::days(7)),
            }),
            now(),
        );
        assert_eq!(state.rate_limited_until, Some(now() + Duration::hours(24)));

        apply_outcome(
            &mut state,
            &Err(CheckError::RateLimited {
                until: Some(now() - Duration::hours(1)),
            }),
            now(),
        );
        assert_eq!(state.rate_limited_until, Some(now() + Duration::minutes(1)));
    }

    #[test]
    fn repeated_failures_back_off_to_a_day() {
        let mut state = State::default();
        let mut waits = Vec::new();
        for _ in 0..7 {
            apply_outcome(&mut state, &Err(CheckError::Unexpected("x".into())), now());
            waits.push((state.retry_after.unwrap() - now()).num_hours());
        }
        assert_eq!(waits, [1, 2, 4, 8, 16, 24, 24]);

        let mut state = State::default();
        apply_outcome(
            &mut state,
            &Err(CheckError::RateLimited { until: None }),
            now(),
        );
        assert_eq!(state.rate_limited_until, Some(now() + Duration::hours(1)));
    }
}
