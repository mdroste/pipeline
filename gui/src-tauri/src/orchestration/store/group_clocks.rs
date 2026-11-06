//! Older runs counted attempts but did not persist the group's elapsed clock.
//! A current or archived attempt proves entry no later than its start time.
//! Do not reopen completed groups or start a future loop iteration early.
use super::{err, Result, Store, TaskRun};
use crate::orchestration::{adapters, definition::Action};
use std::collections::BTreeMap;
impl Store {
    pub(super) fn restore_group_clocks(&self, run: &mut TaskRun) -> Result<bool> {
        let mut steps = Vec::new();
        adapters::flatten(&run.chain.steps, &mut steps);
        let limits: BTreeMap<_, _> = steps
            .iter()
            .filter_map(|s| match &s.action {
                Action::Chain { chain } => {
                    Some((s.id.as_str(), i64::from(chain.limits.deadline_hours) * 3600))
                }
                _ => None,
            })
            .collect();
        if limits.is_empty() {
            return Ok(false);
        }
        let mut starts = BTreeMap::<String, i64>::new();
        let mut record = |address: &str, time: i64| {
            for (slash, _) in address.match_indices('/') {
                let prefix = &address[..slash];
                let group = prefix.rsplit('/').next().unwrap_or(prefix);
                if let Some(hours) = limits.get(group) {
                    if !run
                        .progress
                        .decisions
                        .contains_key(&format!("{prefix}/done"))
                        && !run.progress.group_deadlines.contains_key(prefix)
                    {
                        let end = time.saturating_add(*hours);
                        starts
                            .entry(prefix.into())
                            .and_modify(|old| *old = (*old).min(end))
                            .or_insert(end);
                    }
                }
            }
        };
        for r in run.progress.receipts.values() {
            record(&r.address, r.started_at);
        }
        if run.progress.actions as usize > run.progress.receipts.len() {
            let c = self.connection()?;
            let mut q=c.prepare("SELECT json_extract(a.value,'$.address'),MIN(json_extract(a.value,'$.startedAt')) FROM events e,json_each(CASE WHEN json_valid(e.detail) THEN e.detail ELSE '{}' END,'$.attempts') a WHERE e.run_id=?1 AND e.kind='retry' GROUP BY 1").map_err(err)?;
            let rows = q
                .query_map([&run.id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(err)?;
            for row in rows {
                let (address, time) = row.map_err(err)?;
                record(&address, time);
            }
        }
        let changed = !starts.is_empty();
        run.progress.group_deadlines.extend(starts);
        Ok(changed)
    }
}
