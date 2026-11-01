//! Atomic schedule activation, due occurrences and missed-occurrence coalescing.
use super::{
    err, hash, id, insert_run, now, BlockingRun, Result, Schedule, ScheduleView, Store, TaskRun,
};
use crate::orchestration::{state::Progress, triggers::Trigger};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

impl Store {
    pub fn save_schedule(&self, mut schedule: Schedule, expected: Option<i64>) -> Result<Schedule> {
        schedule.trigger.validate()?;
        crate::orchestration::definition::validate(&schedule.chain)?;
        if !schedule.trigger.recurring() {
            return Err("Use a delayed task for one-time starts".into());
        }
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        if let Some(rev) = expected {
            schedule.revision = rev + 1;
            let changed=tx.execute("UPDATE schedules SET revision=?2,enabled=?3,next_due_at=?4,body=?5 WHERE id=?1 AND revision=?6",params![schedule.id,schedule.revision,schedule.enabled,schedule.next_due_at,serde_json::to_string(&schedule).map_err(err)?,rev]).map_err(err)?;
            if changed != 1 {
                return Err("Schedule changed; reload before editing".into());
            }
        } else {
            tx.execute("INSERT INTO schedules(id,revision,enabled,next_due_at,body) VALUES(?1,?2,?3,?4,?5)",params![schedule.id,schedule.revision,schedule.enabled,schedule.next_due_at,serde_json::to_string(&schedule).map_err(err)?]).map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(schedule)
    }

    pub fn activate_schedule(&self, run: &mut TaskRun, trigger: Trigger) -> Result<()> {
        if run.state != "draft" || !trigger.recurring() {
            return Err("Only a recurring draft can create a schedule".into());
        }
        let schedule = Schedule {
            id: run.id.clone(),
            revision: 0,
            name: run.name.clone(),
            enabled: true,
            chain: run.chain.clone(),
            scope: run.scope.clone(),
            inputs: run.inputs.clone(),
            next_due_at: trigger.next(now() - 1)?,
            trigger,
            created_at: now(),
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        };
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let expected = run.revision;
        run.revision += 1;
        run.state = "finished".into();
        run.reason = Some("Schedule created".into());
        run.updated_at = now();
        run.due_at = None;
        let changed=tx.execute("UPDATE runs SET revision=?2,state=?3,updated_at=?4,body=?5,due_at=NULL WHERE id=?1 AND revision=?6",params![run.id,run.revision,run.state,run.updated_at,serde_json::to_string(run).map_err(err)?,expected]).map_err(err)?;
        if changed != 1 {
            return Err("Task changed; reload before starting".into());
        }
        tx.execute(
            "INSERT INTO schedules(id,revision,enabled,next_due_at,body) VALUES(?1,0,1,?2,?3)",
            params![
                schedule.id,
                schedule.next_due_at,
                serde_json::to_string(&schedule).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.execute("INSERT INTO events(run_id,at,kind,detail) VALUES(?1,?2,'scheduled','Schedule created')",params![run.id,now()]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }

    /// Past runs stay in history, and a run in progress finishes under its own
    /// controls. Only the schedule stops.
    pub fn delete_schedule(&self, id: &str, expected: i64) -> Result<()> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let removed = tx
            .execute(
                "DELETE FROM schedules WHERE id=?1 AND revision=?2",
                params![id, expected],
            )
            .map_err(err)?;
        if removed != 1 {
            return Err("Schedule changed; reload before deleting".into());
        }
        tx.execute("DELETE FROM occurrences WHERE schedule_id=?1", [id])
            .map_err(err)?;
        tx.commit().map_err(err)
    }

    /// Schedules with the unfinished run, if any, that holds back their next one.
    pub fn schedule_views(&self) -> Result<Vec<ScheduleView>> {
        let c = self.connection()?;
        let mut q=c.prepare("SELECT id,state FROM runs WHERE schedule_id=?1 AND state NOT IN ('finished','cancelled','failed') ORDER BY updated_at DESC,id LIMIT 1").map_err(err)?;
        self.schedules()?
            .into_iter()
            .map(|schedule| {
                let blocking_run = q
                    .query_row([&schedule.id], |r| {
                        Ok(BlockingRun {
                            id: r.get(0)?,
                            state: r.get(1)?,
                        })
                    })
                    .optional()
                    .map_err(err)?;
                Ok(ScheduleView {
                    schedule,
                    blocking_run,
                })
            })
            .collect()
    }

    pub fn schedules(&self) -> Result<Vec<Schedule>> {
        let c = self.connection()?;
        let mut s = c
            .prepare("SELECT body FROM schedules ORDER BY id LIMIT 1000")
            .map_err(err)?;
        let bodies = s
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?;
        bodies
            .iter()
            .map(|v| serde_json::from_str(v).map_err(err))
            .collect()
    }

    /// A schedule that can't be read or started waits on its own; the rest fire.
    pub fn fire_schedules(&self, time: i64) -> Result<()> {
        let due = {
            let c = self.connection()?;
            let mut q=c.prepare("SELECT s.body FROM schedules s WHERE s.enabled=1 AND s.next_due_at<=?1 AND NOT EXISTS(SELECT 1 FROM runs r WHERE r.schedule_id=s.id AND r.state NOT IN ('finished','cancelled','failed')) ORDER BY s.next_due_at,s.id LIMIT 32").map_err(err)?;
            let rows = q
                .query_map([time], |r| r.get::<_, String>(0))
                .map_err(err)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(err)?;
            rows
        };
        let mut outcome = Ok(());
        for body in due {
            let fired = serde_json::from_str::<Schedule>(&body)
                .map_err(err)
                .and_then(|schedule| self.fire_schedule(schedule, time));
            outcome = outcome.and(fired);
        }
        outcome
    }

    fn fire_schedule(&self, mut schedule: Schedule, time: i64) -> Result<()> {
        let mut c = self.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        let revision: i64 = tx
            .query_row(
                "SELECT revision FROM schedules WHERE id=?1",
                [&schedule.id],
                |r| r.get(0),
            )
            .map_err(err)?;
        if revision != schedule.revision {
            return Ok(());
        }
        let first = schedule.next_due_at.ok_or("Missing schedule time")?;
        // Arithmetic interval coalescing and a bounded calendar scan avoid replay storms.
        let latest = match &schedule.trigger {
            Trigger::Interval { seconds, .. } => {
                first + (time - first) / (*seconds as i64) * (*seconds as i64)
            }
            _ => {
                let mut latest = first;
                let mut cursor = first.max(time.saturating_sub(8 * 86400));
                for _ in 0..10 {
                    match schedule.trigger.next(cursor)? {
                        Some(v) if v <= time => {
                            latest = v;
                            cursor = v;
                        }
                        _ => break,
                    }
                }
                latest
            }
        };
        let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM runs WHERE schedule_id=?1 AND state NOT IN ('finished','cancelled','failed'))",[&schedule.id],|r|r.get(0)).map_err(err)?;
        if active {
            return Ok(());
        } // Keep the overdue cursor as the single coalesced pending occurrence.
        let operation = format!("schedule-{}-{}-{latest}", schedule.id, schedule.revision);
        let run = TaskRun {
            id: id(),
            mission_id: None,
            revision: 0,
            name: schedule.name.clone(),
            state: "queued".into(),
            reason: None,
            chain: schedule.chain.clone(),
            scope: schedule.scope.clone(),
            inputs: schedule.inputs.clone(),
            progress: Progress::default(),
            created_at: time,
            updated_at: time,
            due_at: Some(time),
            deadline_at: time + schedule.chain.limits.deadline_hours as i64 * 3600,
            schedule_id: Some(schedule.id.clone()),
            occurrence: Some(latest),
        };
        let inserted=tx.execute("INSERT OR IGNORE INTO occurrences(schedule_id,activation,at,run_id,disposition) VALUES(?1,?2,?3,?4,'queued')",params![schedule.id,schedule.revision,latest,run.id]).map_err(err)?;
        if inserted > 0 {
            insert_run(&tx, &run, &operation, &hash(&(&schedule.chain, latest))?)?;
            if latest != first {
                tx.execute("INSERT OR IGNORE INTO occurrences(schedule_id,activation,at,disposition) VALUES(?1,?2,?3,'coalesced')",params![schedule.id,schedule.revision,first]).map_err(err)?;
            }
        }
        schedule.next_due_at = schedule.trigger.next(time)?;
        tx.execute(
            "UPDATE schedules SET next_due_at=?2,body=?3 WHERE id=?1",
            params![
                schedule.id,
                schedule.next_due_at,
                serde_json::to_string(&schedule).map_err(err)?
            ],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }
}
