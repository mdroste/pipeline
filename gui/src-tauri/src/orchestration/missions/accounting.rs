//! Pure cumulative mission budgets and adopted child scope accounting.
use super::model::{Mission, Phase};
use crate::orchestration::store::{now, TaskRun};

pub(crate) fn exhausted(m: &Mission) -> bool {
    m.actions_reserved >= m.definition.budget.max_actions
        || m.active_seconds + 30 > m.definition.budget.active_seconds as u64
        || (m.phase == Phase::Plan && m.rounds.len() >= m.definition.budget.max_rounds as usize)
}

pub(crate) fn account(m: &mut Mission, child: &TaskRun) {
    // A receipt lost in a restart carries the last time the app saw it. Failing
    // that, no action can outlast its timeout, so closed time is never charged.
    let timeout = u64::from(child.chain.limits.action_timeout_secs);
    m.active_seconds = m.active_seconds.saturating_add(
        child
            .progress
            .receipts
            .values()
            .map(|r| {
                (r.finished_at
                    .unwrap_or(now())
                    .saturating_sub(r.started_at)
                    .max(0) as u64)
                    .min(timeout)
            })
            .sum::<u64>(),
    );
    let target = if child.scope.session_id == m.planner_scope.session_id {
        &mut m.planner_scope
    } else if child.scope.session_id == m.challenger_scope.session_id {
        &mut m.challenger_scope
    } else {
        &mut m.scope
    };
    *target = child.scope.clone();
}
