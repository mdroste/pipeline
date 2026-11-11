//! One budget for preparation, review steps, and revision comparison.
//! The run scope carries it across provider tasks and blocking work.
use super::executor::budget::{RunBudget, BUDGET_STOP_PREFIX};
use super::logging::CallUsage;
use crate::settings::Settings;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub(crate) struct State {
    limits: RunBudget,
    started: Instant,
    cost: f64,
    completed_calls: u64,
    in_flight: u64,
}

impl State {
    fn reason(&self) -> Option<String> {
        // Reservations prevent simultaneously scheduled helpers from all
        // passing the same call-count check before their providers start.
        let calls = (self.completed_calls + self.in_flight)
            .max(crate::pipeline::logging::run_usage().provider_attempts);
        super::executor::budget::limit_reason(self.limits, self.started, self.cost, calls)
    }
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|error| error.into_inner())
}

fn current() -> Option<Arc<Mutex<State>>> {
    crate::run_scope::current()
        .review_budget
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

pub(crate) struct Guard {
    scope: Arc<crate::run_scope::RunScope>,
    previous: Option<Arc<Mutex<State>>>,
}

impl Drop for Guard {
    fn drop(&mut self) {
        *self
            .scope
            .review_budget
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = self.previous.take();
    }
}

pub(crate) fn begin(settings: &Settings, started: Instant) -> Guard {
    let scope = crate::run_scope::current();
    let previous = scope
        .review_budget
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(Arc::new(Mutex::new(State {
            limits: RunBudget::from_settings(settings),
            started,
            cost: 0.0,
            completed_calls: 0,
            in_flight: 0,
        })));
    Guard { scope, previous }
}

pub(crate) fn active() -> bool {
    current().is_some()
}

pub(crate) fn reason() -> Option<String> {
    current().and_then(|state| lock(&state).reason())
}

pub(crate) fn stopped(error: &str) -> bool {
    error.contains(BUDGET_STOP_PREFIX)
}

pub(crate) fn check() -> Result<(), String> {
    reason().map_or(Ok(()), |reason| {
        Err(format!("{BUDGET_STOP_PREFIX} {reason}"))
    })
}

/// A reservation lives until a provider returns or its future is dropped.
struct Reservation {
    budget: Arc<Mutex<State>>,
    calls: AtomicU64,
    started: AtomicU64,
}

tokio::task_local! {
    static INVOCATION: Option<Arc<Reservation>>;
}

pub(crate) struct Permit(Option<Arc<Reservation>>);

pub(crate) fn admit() -> Result<Permit, String> {
    let state = current();
    if let Some(state) = &state {
        let mut budget = lock(state);
        if let Some(reason) = budget.reason() {
            return Err(format!("{BUDGET_STOP_PREFIX} {reason}"));
        }
        budget.in_flight += 1;
    }
    Ok(Permit(state.map(|budget| {
        Arc::new(Reservation {
            budget,
            calls: AtomicU64::new(1),
            started: AtomicU64::new(0),
        })
    })))
}

/// The first attempt uses the dispatch reservation. Internal warm-ups and
/// capability retries need additional capacity, reserved atomically too.
pub(crate) fn before_attempt() -> Result<(), String> {
    INVOCATION
        .try_with(|reservation| {
            let Some(reservation) = reservation else {
                return Ok(());
            };
            if reservation.started.fetch_add(1, Ordering::SeqCst) == 0 {
                return Ok(());
            }
            let mut budget = lock(&reservation.budget);
            if let Some(reason) = budget.reason() {
                return Err(format!("{BUDGET_STOP_PREFIX} {reason}"));
            }
            budget.in_flight += 1;
            reservation.calls.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .unwrap_or(Ok(()))
}

impl Permit {
    pub(crate) async fn scope<F: std::future::Future>(&self, future: F) -> F::Output {
        INVOCATION.scope(self.0.clone(), future).await
    }

    pub(crate) fn complete(
        mut self,
        provider: &str,
        model: &str,
        usage: CallUsage,
        settings: &Settings,
    ) {
        if let Some(reservation) = self.0.take() {
            let mut budget = lock(&reservation.budget);
            budget.in_flight -= reservation.calls.load(Ordering::SeqCst);
            budget.completed_calls += usage.provider_attempts;
            let output = crate::models::StepOutput {
                provider: provider.to_string(),
                model: model.to_string(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                cache_write_input_tokens: usage.cache_write_input_tokens,
                ..Default::default()
            };
            budget.cost += crate::output::api_cost_of(&output, settings).0;
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(reservation) = &self.0 {
            let reserved = reservation.calls.load(Ordering::SeqCst);
            let mut budget = lock(&reservation.budget);
            budget.in_flight -= reserved;
            // Dropping a future releases pending capacity, not attempts that
            // already started. Keep those spent before another helper admits.
            budget.completed_calls += reserved.min(reservation.started.load(Ordering::SeqCst));
        }
    }
}

#[cfg(test)]
mod tests;
