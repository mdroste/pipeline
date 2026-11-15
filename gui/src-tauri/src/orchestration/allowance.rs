//! One transaction admits each nested provider attempt against its parent's
//! remaining allowance and every enclosing group's cap. Ambiguous sends keep
//! their reservation, including after restart or a Retry with a new operation.
use super::{
    adapters,
    definition::Action,
    discovery,
    state::Ready,
    store::{err, Result, Store, TaskRun},
};
use crate::usage_allowance::{Allowance, Class, Meter};
use rusqlite::{params, TransactionBehavior};
use std::sync::Arc;

struct Ledger {
    store: Store,
    task: String,
    owner: String,
    address: String,
    operation: String,
    cap: Option<u32>,
    groups: Vec<(String, u32)>,
}

pub(super) fn meter(
    store: &Store,
    run: &TaskRun,
    ready: &Ready,
    operation: &str,
) -> Result<Allowance> {
    let owner = match &run.mission_id {
        Some(id) => id.clone(),
        None => discovery::storage::owner(store, &run.id)?.unwrap_or_else(|| run.id.clone()),
    };
    let mut steps = Vec::new();
    adapters::flatten(&run.chain.steps, &mut steps);
    let groups = ready
        .groups
        .iter()
        .filter_map(|(prefix, _)| {
            let id = prefix.trim_end_matches('/').rsplit('/').next()?;
            let Action::Chain { chain } = &steps.iter().find(|s| s.id == id)?.action else {
                return None;
            };
            Some((prefix.clone(), chain.limits.max_api_cost_cents?))
        })
        .collect();
    Ok(Arc::new(Ledger {
        store: store.clone(),
        owner,
        task: run.id.clone(),
        address: ready.address.clone(),
        operation: operation.into(),
        cap: run.chain.limits.max_api_cost_cents,
        groups,
    }))
}
fn price(model: &str, t: [u64; 4]) -> Option<i64> {
    crate::output::estimate_cost(model, t[0], t[1], t[2], t[3])
        .map(|usd| (usd * 1_000_000.0).ceil() as i64)
}
impl Meter for Ledger {
    fn reserve(&self, class: Class, model: &str, input: u64, output: u64) -> Result<i64> {
        let estimate = if class == Class::Api {
            price(model, [input, output, 0, 0])
        } else {
            Some(0)
        };
        if estimate.is_none() && (self.cap.is_some() || !self.groups.is_empty()) {
            return Err(format!(
                "API allowance: no list price is available for {model}; the request was not sent."
            ));
        }
        let mut c = self.store.connection()?;
        let tx = c
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(err)?;
        if class == Class::Api {
            let check = |used: i64, cap: u32| -> Result<()> {
                let max = i64::from(cap) * 10000;
                if used >= max || used.saturating_add(estimate.unwrap_or(0)) > max {
                    return Err(format!(
                        "API allowance of ${:.2} would be exceeded; the request was not sent.",
                        f64::from(cap) / 100.0
                    ));
                }
                Ok(())
            };
            if let Some(cap) = self.cap {
                let used = tx
                    .query_row(
                        "SELECT COALESCE(SUM(charge),0) FROM usage_calls WHERE owner_id=?1",
                        [&self.owner],
                        |r| r.get(0),
                    )
                    .map_err(err)?;
                check(used, cap)?;
            }
            for (prefix, cap) in &self.groups {
                let used=tx.query_row("SELECT COALESCE(SUM(charge),0) FROM usage_calls WHERE task_id=?1 AND substr(address,1,length(?2))=?2",params![self.task,prefix],|r|r.get(0)).map_err(err)?;
                check(used, *cap)?;
            }
        }
        let class = match class {
            Class::Subscription => "subscription",
            Class::Local => "local",
            Class::Api if estimate.is_some() => "api",
            Class::Api => "unpriced",
        };
        tx.execute("INSERT INTO usage_calls(task_id,owner_id,address,operation,class,charge) VALUES(?1,?2,?3,?4,?5,?6)",params![self.task,self.owner,self.address,self.operation,class,estimate]).map_err(err)?;
        let id = tx.last_insert_rowid();
        tx.commit().map_err(err)?;
        Ok(id)
    }
    fn settle(&self, id: i64, model: &str, t: [u64; 4]) -> Result<()> {
        self.store.connection()?.execute("UPDATE usage_calls SET settled=1, charge=CASE WHEN class='api' THEN COALESCE(?1,charge) ELSE charge END,input_tokens=?2,output_tokens=?3 WHERE id=?4 AND operation=?5 AND settled=0",params![price(model,t),i64::try_from(t[0]).map_err(err)?,i64::try_from(t[1]).map_err(err)?,id,self.operation]).map_err(err)?;
        Ok(())
    }
}

pub(crate) fn summary(store: &Store, owner: &str) -> Result<serde_json::Value> {
    store.connection()?.query_row("SELECT COUNT(*) FILTER(WHERE class='api'),COUNT(*) FILTER(WHERE class='unpriced'),COUNT(*) FILTER(WHERE class='subscription'),COUNT(*) FILTER(WHERE class='local'),COALESCE(SUM(charge),0),COALESCE(SUM(charge) FILTER(WHERE settled=0),0),COALESCE(SUM(input_tokens),0),COALESCE(SUM(output_tokens),0) FROM usage_calls WHERE owner_id=?1 OR task_id=?1",[owner],|r|{
        let mut v=serde_json::json!({});
        for (i,key) in ["pricedCalls","unpricedCalls","subscriptionUnits","localCalls","chargedMicroUsd","heldMicroUsd","inputTokens","outputTokens"].iter().enumerate() {v[key]=r.get::<_,i64>(i)?.into()}
        Ok(v)
    }).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        orchestration::{
            definition::{Binding, Chain, Limits, Step},
            state::{advance, Advance},
            store::Scope,
        },
        pipeline::executor::budget_dispatch,
        run_scope, usage_allowance,
    };
    use serde_json::json;

    #[test]
    fn parallel_paid_requests_compete_for_the_same_group_allowance() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let inner = Chain {
            schema_version: 1,
            name: "nested".into(),
            description: String::new(),
            limits: Limits {
                max_api_cost_cents: Some(2),
                ..Default::default()
            },
            steps: vec![Step {
                id: "work".into(),
                label: "Work".into(),
                action: Action::Workspace {
                    prompt: "work".into(),
                    model: None,
                    effort: None,
                },
            }],
        };
        let root = Chain {
            schema_version: 1,
            name: "outer".into(),
            description: String::new(),
            limits: Default::default(),
            steps: vec![Step {
                id: "g".into(),
                label: "Group".into(),
                action: Action::Chain {
                    chain: Box::new(inner),
                },
            }],
        };
        let mut run = store
            .create("group-spend", root, Scope::default(), json!({}), 0, true)
            .unwrap();
        let Advance::Ready(a) = advance(&run.chain, &mut run.progress, &json!({})) else {
            panic!()
        };
        let m = meter(&store, &run, &a[0], "attempt").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let admitted = std::thread::scope(|s| {
            let send = || {
                barrier.wait();
                m.reserve(Class::Api, "claude-sonnet", 1000, 1000).is_ok()
            };
            let a = s.spawn(send);
            let b = s.spawn(send);
            u32::from(a.join().unwrap()) + u32::from(b.join().unwrap())
        });
        assert_eq!(admitted, 1);
        assert_eq!(summary(&store, &run.id).unwrap()["pricedCalls"], 1);
    }

    #[tokio::test]
    async fn a_nested_review_cannot_reuse_a_portfolios_reserved_money_after_restart_or_retry() {
        let temp = tempfile::tempdir().unwrap();
        let store = Store::open(temp.path()).unwrap();
        let task = |name: &str| {
            let chain = Chain {
                schema_version: 1,
                name: "child".into(),
                description: String::new(),
                limits: Limits {
                    max_api_cost_cents: Some(2),
                    ..Default::default()
                },
                steps: vec![Step {
                    id: "review".into(),
                    label: "Review".into(),
                    action: Action::Review {
                        profile_id: "p".into(),
                        input: Binding::Literal {
                            value: json!("frozen"),
                        },
                        interpretation: "document".into(),
                        variables: Default::default(),
                    },
                }],
            };
            store
                .create(name, chain, Scope::default(), json!({}), 0, true)
                .unwrap()
        };
        let mut first = task("first");
        let mut retry = task("retry");
        let c = store.connection().unwrap();
        c.execute("INSERT INTO discovery_runs(id,operation,fingerprint,revision,state,workspace_id,updated_at,deadline_at,body) VALUES('portfolio','op','fp',1,'running','ws',0,1,'{}')",[]).unwrap();
        for run in [&first, &retry] {
            c.execute("INSERT INTO discovery_children(task_id,run_id,phase) VALUES(?1,'portfolio','review')",[&run.id]).unwrap();
        }
        let Advance::Ready(a) = advance(&first.chain, &mut first.progress, &json!({})) else {
            panic!()
        };
        let meter1 = meter(&store, &first, &a[0], "original").unwrap();
        let scope = run_scope::ActiveScope::begin("first-review");
        scope.scope().allowance.set(meter1).ok().unwrap();
        run_scope::scoped(scope.scope().clone(), async {
            // A lost reply: no settlement. An independent child, a new Review
            // scope, and a fresh SQLite connection cannot spend its estimate.
            budget_dispatch::reserve(Some(("claude-sonnet", 1000, 1000)), true).unwrap();
        })
        .await;
        drop(scope);
        let reopened = Store::open(temp.path()).unwrap();
        let Advance::Ready(b) = advance(&retry.chain, &mut retry.progress, &json!({})) else {
            panic!()
        };
        let meter2 = meter(&reopened, &retry, &b[0], "replacement").unwrap();
        let scope = run_scope::ActiveScope::begin("retried-review");
        scope.scope().allowance.set(meter2).ok().unwrap();
        run_scope::scoped(scope.scope().clone(), async {
            assert!(budget_dispatch::reserve(Some(("claude-sonnet", 1000, 1000)), true).is_err());
            assert!(budget_dispatch::reserve(Some(("unknown-price", 1, 1)), true).is_err());
            budget_dispatch::reserve_as(
                Some(("unknown-local", 10000, 10000)),
                true,
                usage_allowance::Class::Local,
            )
            .unwrap();
            budget_dispatch::reserve(None, true).unwrap();
            // Known usage releases only its own reservation; repeated
            // reconciliation cannot subtract the first child's held spend.
            let r = budget_dispatch::reserve(Some(("claude-sonnet", 20, 20)), true).unwrap();
            r.settle("claude-sonnet", 10, 5, 0, 0);
        })
        .await;
        let v = summary(&reopened, "portfolio").unwrap();
        assert_eq!(v["pricedCalls"], 2);
        assert_eq!(v["localCalls"], 1);
        assert_eq!(v["subscriptionUnits"], 1);
        assert_eq!(v["inputTokens"], 10);
        assert!(v["heldMicroUsd"].as_i64().unwrap() > 0);
        assert!(v["chargedMicroUsd"].as_i64().unwrap() > v["heldMicroUsd"].as_i64().unwrap());
    }
}
