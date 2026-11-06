//! Per-run admission for provider submissions. A reservation holds estimated
//! list-price spend until usage arrives. An ambiguous request keeps its estimate.
use super::budget::{RunBudget, BUDGET_STOP_PREFIX};
use crate::{output, run_scope};
use std::time::Instant;

#[derive(Default)]
pub(crate) struct DispatchBudget {
    limits: RunBudget,
    started: Option<Instant>,
    attempts: u64,
    spend: f64,
    generation: u64,
}

impl DispatchBudget {
    pub(crate) fn new(limits: RunBudget, generation: u64) -> Self {
        Self {
            limits,
            generation,
            started: Some(Instant::now()),
            ..Self::default()
        }
    }

    fn admit(&mut self, api: Option<(&str, u64, u64)>) -> Result<f64, String> {
        // Standalone tooling and legacy callers without a frozen Review
        // retain their telemetry but share no per-run allowance.
        if self.started.is_none() {
            return Ok(0.0);
        }
        if let Some(max) = self.limits.max_calls {
            if self.attempts >= u64::from(max) {
                return Err(format!("{BUDGET_STOP_PREFIX} limit of {max} model calls."));
            }
        }
        if let (Some(max), Some(start)) = (self.limits.max_minutes, self.started) {
            if start.elapsed().as_secs() >= u64::from(max) * 60 {
                return Err(format!("{BUDGET_STOP_PREFIX} time limit of {max} minutes."));
            }
        }
        let mut estimate = 0.0;
        if let (Some(max), Some((model, input, output))) = (self.limits.max_cost_usd, api) {
            estimate = output::estimate_cost(model, input, output, 0, 0).ok_or_else(|| {
                format!(
                "{BUDGET_STOP_PREFIX} cost control: no API list price is available for '{model}'."
            )
            })?;
            if self.spend + estimate > max || self.spend >= max {
                return Err(format!("{BUDGET_STOP_PREFIX} cost limit of ${max:.2} (including this request's estimated ${estimate:.4})."));
            }
        }
        self.attempts += 1;
        self.spend += estimate;
        Ok(estimate)
    }
}

pub(crate) struct Reservation {
    caller: Option<crate::usage_allowance::Hold>,
    scope: std::sync::Arc<run_scope::RunScope>,
    generation: u64,
    estimate: f64,
}

impl Reservation {
    pub(crate) fn settle(self, model: &str, input: u64, output: u64, cached: u64, write: u64) {
        if let Some(hold) = self.caller {
            hold.settle(model, [input, output, cached, write]);
        }
        if self.estimate > 0.0 {
            if let Some(actual) = output::estimate_cost(model, input, output, cached, write) {
                let mut budget = self
                    .scope
                    .dispatch_budget
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if budget.generation == self.generation {
                    budget.spend = (budget.spend - self.estimate).max(0.0) + actual;
                }
            }
        }
    }
}

pub(crate) fn admitted_attempts() -> u64 {
    run_scope::current()
        .dispatch_budget
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .attempts
}

pub(crate) fn reserve(
    api: Option<(&str, u64, u64)>,
    logical_start: bool,
) -> Result<Reservation, String> {
    reserve_as(
        api,
        logical_start,
        if api.is_some() {
            crate::usage_allowance::Class::Api
        } else {
            crate::usage_allowance::Class::Subscription
        },
    )
}

pub(crate) fn reserve_as(
    api: Option<(&str, u64, u64)>,
    logical_start: bool,
    class: crate::usage_allowance::Class,
) -> Result<Reservation, String> {
    let scope = run_scope::current();
    let (estimate, generation) = {
        let mut budget = scope
            .dispatch_budget
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        (
            budget.admit(if class == crate::usage_allowance::Class::Local {
                None
            } else {
                api
            })?,
            budget.generation,
        )
    };
    let (model, input, output) = api.unwrap_or(("", 0, 0));
    let caller =
        crate::usage_allowance::Hold::reserve(scope.allowance.get(), class, model, input, output)
            .map_err(|e| format!("{BUDGET_STOP_PREFIX} {e}"))?;
    if logical_start {
        crate::pipeline::logging::record_provider_attempt();
    }
    Ok(Reservation {
        caller,
        estimate,
        generation,
        scope,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parallel_dispatch_cannot_exceed_the_calls_left_after_orientation() {
        let active = run_scope::ActiveScope::begin("dispatch-admission");
        run_scope::scoped(active.scope().clone(), async {
            *run_scope::current().dispatch_budget.lock().unwrap() = DispatchBudget::new(
                RunBudget {
                    max_calls: Some(2),
                    ..Default::default()
                },
                1,
            );
            reserve(None, true).unwrap(); // orientation
            let mut units = tokio::task::JoinSet::new();
            for _ in 0..10 {
                run_scope::spawn_in(&mut units, async { reserve(None, true).is_ok() });
            }
            let mut dispatched = 0;
            while let Some(unit) = units.join_next().await {
                dispatched += usize::from(unit.unwrap());
            }
            assert_eq!(dispatched, 1);
            assert_eq!(run_scope::current().usage().provider_attempts, 2);
            assert!(reserve(None, true).is_err()); // retry, backup or reconciliation
        })
        .await;
    }

    #[test]
    fn spend_is_reserved_before_admitting_another_request() {
        let limit = output::estimate_cost("claude-sonnet", 1000, 1000, 0, 0).unwrap();
        let mut budget = DispatchBudget::new(
            RunBudget {
                max_cost_usd: Some(limit),
                ..Default::default()
            },
            1,
        );
        assert!(budget.admit(Some(("unknown-price", 1, 1))).is_err());
        assert!(budget.admit(Some(("claude-sonnet", 1000, 1000))).is_ok());
        assert!(budget.admit(Some(("claude-sonnet", 1, 1))).is_err());
    }
}
