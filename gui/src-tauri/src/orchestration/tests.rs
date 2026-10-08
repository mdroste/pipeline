use super::*;
use definition::*;
use serde_json::json;
use triggers::*;
fn chain(steps: Vec<Step>) -> Chain {
    Chain {
        schema_version: 1,
        name: "Test".into(),
        description: String::new(),
        steps,
        limits: Limits::default(),
    }
}
fn step(id: &str, action: Action) -> Step {
    Step {
        id: id.into(),
        label: id.into(),
        action,
    }
}
fn delay(id: &str) -> Step {
    step(id, Action::Delay { seconds: 60 })
}
fn done(p: &mut state::Progress, ready: &state::Ready, value: Value) {
    p.receipts.insert(
        ready.address.clone(),
        Receipt {
            sequence: 0,
            address: ready.address.clone(),
            step_id: ready.step.id.clone(),
            label: ready.step.label.clone(),
            state: "completed".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: Some(1),
            wake_at: None,
            output: Some(value.clone()),
            error: None,
            child: None,
        },
    );
    p.outputs[&ready.step.id] = value;
}
fn output(step: &str, pointer: &str) -> Binding {
    Binding::Output {
        step: step.into(),
        pointer: pointer.into(),
    }
}
#[test]
fn strict_chain_roundtrip() {
    let c = chain(vec![delay("a")]);
    validate(&c).unwrap();
    let json = serde_json::to_string(&c).unwrap();
    let decoded: Chain = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, c);
}
#[test]
fn missing_findings_are_unknown() {
    let condition = Condition::LessThan {
        left: output("review", "/highPriorityCount"),
        right: 1.,
    };
    assert_eq!(condition.evaluate(&json!({}), &json!({})), Truth::Unknown);
    assert_eq!(
        condition.evaluate(&json!({"review":{"highPriorityCount":0}}), &json!({})),
        Truth::True
    );
}
#[test]
fn stopped_loop_never_dispatches_an_extra_revision() {
    let c = chain(vec![step(
        "loop",
        Action::Repeat {
            max_iterations: 3,
            until: Condition::Equals {
                left: output("review", "/complete"),
                right: json!(true),
            },
            steps: vec![delay("review")],
        },
    )]);
    let mut p = state::Progress::default();
    let Advance::Ready(first) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(first[0].address, "loop/0/review");
    done(&mut p, &first[0], json!({"complete":true}));
    assert!(matches!(
        state::advance(&c, &mut p, &json!({})),
        Advance::Finished
    ));
    assert_eq!(p.receipts.len(), 1);
}
#[test]
fn loop_limit_is_recorded_and_replay_does_not_rerun() {
    let c = chain(vec![step(
        "loop",
        Action::Repeat {
            max_iterations: 2,
            until: Condition::Equals {
                left: output("review", "/complete"),
                right: json!(true),
            },
            steps: vec![delay("review")],
        },
    )]);
    let mut p = state::Progress::default();
    for i in 0..2 {
        let Advance::Ready(ready) = state::advance(&c, &mut p, &json!({})) else {
            panic!()
        };
        assert_eq!(ready[0].address, format!("loop/{i}/review"));
        done(&mut p, &ready[0], json!({"complete":false}));
    }
    assert!(matches!(
        state::advance(&c, &mut p, &json!({})),
        Advance::Finished
    ));
    assert!(p.limit_reached);
    assert!(matches!(
        state::advance(&c, &mut p, &json!({})),
        Advance::Finished
    ));
}
#[test]
fn parallel_branches_join_before_next_step() {
    let c = chain(vec![
        step(
            "parallel",
            Action::Parallel {
                branches: vec![vec![delay("a")], vec![delay("b")]],
            },
        ),
        delay("after"),
    ]);
    let mut p = state::Progress::default();
    let Advance::Ready(first) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(first.len(), 2);
    done(&mut p, &first[0], json!({}));
    let Advance::Ready(second) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].step.id, "b");
    done(&mut p, &second[0], json!({}));
    let Advance::Ready(last) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(last[0].step.id, "after");
}
#[test]
fn condition_choice_is_persisted() {
    let c = chain(vec![step(
        "branch",
        Action::If {
            condition: Condition::Equals {
                left: Binding::Input {
                    key: "answer".into(),
                },
                right: json!(true),
            },
            then_steps: vec![delay("yes")],
            else_steps: vec![delay("no")],
        },
    )]);
    let mut p = state::Progress::default();
    assert!(matches!(
        state::advance(&c, &mut p, &json!({})),
        Advance::Attention(_)
    ));
    let Advance::Ready(a) = state::advance(&c, &mut p, &json!({"answer":true})) else {
        panic!()
    };
    assert_eq!(a[0].step.id, "yes");
    let Advance::Ready(b) = state::advance(&c, &mut p, &json!({"answer":false})) else {
        panic!()
    };
    assert_eq!(b[0].step.id, "yes");
}
#[test]
fn duplicate_ids_and_unbounded_loops_rejected() {
    let c = chain(vec![delay("same"), delay("same")]);
    assert!(validate(&c).is_err());
    let c = chain(vec![step(
        "repeat",
        Action::Repeat {
            max_iterations: 0,
            until: Condition::Exists {
                value: Binding::Input { key: "x".into() },
            },
            steps: vec![delay("x")],
        },
    )]);
    assert!(validate(&c).is_err());
}
#[test]
fn input_signal_is_consumed_once_and_cannot_resume_cancelled_task() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = s
        .create(
            "create",
            chain(vec![step(
                "answer",
                Action::Input {
                    prompt: "Which calibration?".into(),
                    timeout_seconds: None,
                },
            )]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    run.state = "waiting".into();
    run.progress.receipts.insert(
        "answer".into(),
        Receipt {
            sequence: 0,
            address: "answer".into(),
            step_id: "answer".into(),
            label: "answer".into(),
            state: "input".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut run, "wait", "waiting").unwrap();
    let first = s
        .signal(&run.id, "answer", "signal", json!("baseline"))
        .unwrap();
    let second = s
        .signal(&run.id, "answer", "signal", json!("baseline"))
        .unwrap();
    assert_eq!(first.revision, second.revision);
    assert_eq!(first.progress.outputs["answer"], "baseline");
    assert!(s
        .signal(&run.id, "answer", "different", json!("alternative"))
        .is_err());
}
#[test]
fn store_recovery_preserves_uncertain_actions_without_replay() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let c = chain(vec![delay("one")]);
    let mut run = s
        .create("create", c.clone(), Scope::default(), json!({}), 0, true)
        .unwrap();
    let duplicate = s
        .create("create", c, Scope::default(), json!({}), 0, true)
        .unwrap();
    assert_eq!(run.id, duplicate.id);
    run.state = "running".into();
    run.progress.receipts.insert(
        "one".into(),
        Receipt {
            sequence: 0,
            address: "one".into(),
            step_id: "one".into(),
            label: "one".into(),
            state: "running".into(),
            operation: store::id(),
            started_at: 0,
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut run, "launch", "launched").unwrap();
    s.recover().unwrap();
    let recovered = s.get(&run.id).unwrap();
    assert_eq!(recovered.state, "attention");
    assert_eq!(recovered.progress.receipts["one"].state, "unknown");
    assert!(s.ready_ids(now()).unwrap().is_empty());
}
#[test]
fn stale_revision_cannot_overwrite_input() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut a = s
        .create(
            "c",
            chain(vec![delay("x")]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    let mut b = a.clone();
    s.save(&mut a, "pause", "a").unwrap();
    assert!(s.save(&mut b, "resume", "b").is_err());
}
#[test]
fn calendar_skips_spring_gap_and_uses_first_autumn_fold() {
    use chrono::{TimeZone, Utc};
    let spring = Trigger::Calendar {
        timezone: "America/New_York".into(),
        hour: 2,
        minute: 30,
        weekdays: vec![6],
    };
    let after = Utc
        .with_ymd_and_hms(2026, 3, 8, 0, 0, 0)
        .unwrap()
        .timestamp();
    assert_eq!(
        spring.next(after).unwrap(),
        Some(
            Utc.with_ymd_and_hms(2026, 3, 15, 6, 30, 0)
                .unwrap()
                .timestamp()
        )
    );
    let autumn = Trigger::Calendar {
        timezone: "America/New_York".into(),
        hour: 1,
        minute: 30,
        weekdays: vec![6],
    };
    let after = Utc
        .with_ymd_and_hms(2026, 11, 1, 0, 0, 0)
        .unwrap()
        .timestamp();
    let first = autumn.next(after).unwrap().unwrap();
    assert_eq!(
        first,
        Utc.with_ymd_and_hms(2026, 11, 1, 5, 30, 0)
            .unwrap()
            .timestamp()
    );
    assert_eq!(
        autumn.next(first).unwrap(),
        Some(
            Utc.with_ymd_and_hms(2026, 11, 8, 6, 30, 0)
                .unwrap()
                .timestamp()
        )
    );
}
#[test]
fn missed_intervals_coalesce_and_overlap_does_not_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    s.save_schedule(
        Schedule {
            id: "schedule".into(),
            revision: 0,
            name: "Scheduled".into(),
            enabled: true,
            chain: chain(vec![delay("x")]),
            scope: Scope::default(),
            inputs: json!({}),
            trigger: Trigger::Interval {
                seconds: 60,
                anchor: 0,
            },
            next_due_at: Some(60),
            created_at: 0,
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        },
        None,
    )
    .unwrap();
    s.fire_schedules(600).unwrap();
    s.fire_schedules(1200).unwrap();
    let runs = s.list("active", None, 0).unwrap();
    assert_eq!(runs.len(), 1);
    let run = s.get(&runs[0].id).unwrap();
    assert_eq!(run.occurrence, Some(600));
}
#[test]
fn blocked_schedule_page_does_not_starve_later_eligible_schedule() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    for index in 0..33 {
        s.save_schedule(
            Schedule {
                id: format!("schedule-{index:02}"),
                revision: 0,
                name: "Scheduled".into(),
                enabled: true,
                chain: chain(vec![delay("x")]),
                scope: Scope::default(),
                inputs: json!({}),
                trigger: Trigger::Interval {
                    seconds: 60,
                    anchor: 0,
                },
                next_due_at: Some(60),
                created_at: 0,
                tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
            },
            None,
        )
        .unwrap();
    }
    s.fire_schedules(600).unwrap();
    let c = s.connection().unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        32
    );
    // The earlier 32 cursors are overdue again, but all have live occurrences.
    s.fire_schedules(1200).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        33
    );
    assert_eq!(
        c.query_row(
            "SELECT at FROM occurrences WHERE schedule_id='schedule-32' AND disposition='queued'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1200
    );
    assert_eq!(s.schedules().unwrap()[0].next_due_at, Some(660));
    s.fire_schedules(1800).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        33
    );
    // Releasing one blocked occurrence creates only its coalesced latest run.
    let run_id: String = c
        .query_row(
            "SELECT id FROM runs WHERE schedule_id='schedule-00'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let mut run = s.get(&run_id).unwrap();
    run.state = "finished".into();
    s.save(&mut run, "finished", "Finished").unwrap();
    s.fire_schedules(1800).unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        34
    );
    assert_eq!(c.query_row("SELECT MAX(at) FROM occurrences WHERE schedule_id='schedule-00' AND disposition='queued'", [], |r| r.get::<_, i64>(0)).unwrap(), 1800);
}
#[test]
fn prompt_bindings_fail_closed() {
    assert_eq!(
        render_prompt(
            "Use {{output:paper#/text}} and {{input:idea}}",
            &json!({"paper":{"text":"Draft"}}),
            &json!({"idea":"Idea"})
        )
        .unwrap(),
        "Use Draft and Idea"
    );
    assert!(render_prompt("{{output:missing}}", &json!({}), &json!({})).is_err());
}

#[test]
fn while_can_skip_its_body() {
    let c = chain(vec![step(
        "loop",
        Action::While {
            max_iterations: 2,
            condition: Condition::Equals {
                left: Binding::Input {
                    key: "continue".into(),
                },
                right: json!(true),
            },
            steps: vec![delay("work")],
        },
    )]);
    assert!(matches!(
        state::advance(
            &c,
            &mut state::Progress::default(),
            &json!({"continue":false})
        ),
        Advance::Finished
    ));
}
#[test]
fn unknown_action_fields_are_rejected() {
    assert!(serde_json::from_value::<Chain>(json!({"schemaVersion":1,"name":"Typo","steps":[{"id":"a","label":"Wait","kind":"delay","seconds":1,"second":2}]})).is_err());
}
#[test]
fn invalid_output_references_are_rejected() {
    let c = chain(vec![step(
        "paper",
        Action::Snapshot {
            input: output("missing", ""),
            filename: "paper.md".into(),
            require_change: false,
        },
    )]);
    assert!(validate(&c).unwrap_err().contains("Unknown output"));
}
#[test]
fn missing_or_null_text_is_never_sent_as_an_empty_paper() {
    assert!(render_prompt(
        "Revise {{output:review#/paperText}}",
        &json!({"review":{"paperText":null}}),
        &json!({})
    )
    .is_err());
}

#[test]
fn output_availability_rejects_cross_branch_self_and_forward_bindings() {
    let consumer = step(
        "consumer",
        Action::If {
            condition: Condition::Equals {
                left: output("producer", ""),
                right: json!(true),
            },
            then_steps: vec![delay("yes")],
            else_steps: vec![],
        },
    );
    let c = chain(vec![step(
        "parallel",
        Action::Parallel {
            branches: vec![vec![delay("producer")], vec![consumer]],
        },
    )]);
    assert!(validate(&c).unwrap_err().contains("producer"));
    let bindings = [
        Action::Deliver {
            input: output("producer", ""),
        },
        Action::Snapshot {
            input: output("producer", ""),
            filename: "paper.md".into(),
            require_change: false,
        },
        Action::Review {
            input: output("producer", ""),
            profile_id: "test".into(),
            interpretation: "document".into(),
            variables: Default::default(),
        },
        Action::Workspace {
            prompt: "Read {{output:producer#/text}}".into(),
            model: None,
            effort: None,
        },
        Action::ForEach {
            input: output("producer", ""),
            max_items: 2,
            steps: vec![delay("inner")],
        },
        Action::Deliver {
            input: Binding::FirstAvailable {
                values: vec![
                    output("producer", ""),
                    Binding::Literal {
                        value: json!("fallback"),
                    },
                ],
            },
        },
    ];
    for action in bindings {
        assert!(validate(&chain(vec![
            step("consumer", action.clone()),
            delay("producer")
        ]))
        .unwrap_err()
        .contains("producer"));
        assert!(validate(&chain(vec![step("producer", action.clone())]))
            .unwrap_err()
            .contains("producer"));
        assert!(validate(&chain(vec![step(
            "parallel",
            Action::Parallel {
                branches: vec![vec![delay("producer")], vec![step("consumer", action)]],
            }
        )]))
        .unwrap_err()
        .contains("producer"));
    }
}

#[test]
fn output_availability_preserves_parallel_join_and_embedded_sequence() {
    let c = chain(vec![
        step(
            "parallel",
            Action::Parallel {
                branches: vec![vec![delay("a")], vec![delay("b")]],
            },
        ),
        step(
            "consumer",
            Action::If {
                condition: Condition::Equals {
                    left: output("a", ""),
                    right: json!(true),
                },
                then_steps: vec![step(
                    "deliver",
                    Action::Deliver {
                        input: output("b", ""),
                    },
                )],
                else_steps: vec![],
            },
        ),
        step(
            "embedded",
            Action::Chain {
                chain: Box::new(chain(vec![step(
                    "read",
                    Action::Workspace {
                        prompt: "{{output:b}}".into(),
                        model: None,
                        effort: None,
                    },
                )])),
            },
        ),
    ]);
    validate(&c).unwrap();
    let mut p = state::Progress::default();
    let Advance::Ready(ready) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(ready.len(), 2);
    for pending in ready {
        done(&mut p, &pending, json!(true));
    }
    let Advance::Ready(ready) = state::advance(&c, &mut p, &json!({})) else {
        panic!()
    };
    assert_eq!(ready[0].step.id, "deliver");
}

#[test]
fn output_availability_preserves_guarded_review_iterations() {
    let c: Chain = serde_json::from_str(include_str!("fixtures/review-chain.json")).unwrap();
    validate(&c).unwrap();
    let mut p = state::Progress::default();
    let mut seen = Vec::new();
    loop {
        match state::advance(&c, &mut p, &json!({})) {
            Advance::Ready(ready) => {
                for pending in ready {
                    let revisited = seen.iter().any(|id| id == "review");
                    seen.push(pending.step.id.clone());
                    done(
                        &mut p,
                        &pending,
                        json!({"complete":revisited,"highPriorityCount":0,"unknownPriorityCount":0}),
                    );
                }
            }
            Advance::Finished => break,
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        seen,
        [
            "draft",
            "initialPaper",
            "review",
            "revise",
            "revisedPaper",
            "review",
            "result"
        ]
    );
}

#[test]
fn output_availability_checks_loop_entry_and_sibling_outputs_inside_loops() {
    let c = chain(vec![step(
        "loop",
        Action::Repeat {
            max_iterations: 2,
            until: Condition::Exists {
                value: output("producer", ""),
            },
            steps: vec![
                step(
                    "consumer",
                    Action::Deliver {
                        input: output("producer", ""),
                    },
                ),
                delay("producer"),
            ],
        },
    )]);
    assert!(validate(&c).unwrap_err().contains("producer"));
    let c = chain(vec![step(
        "loop",
        Action::While {
            max_iterations: 2,
            condition: Condition::Equals {
                left: output("producer", ""),
                right: json!(true),
            },
            steps: vec![delay("producer")],
        },
    )]);
    assert!(validate(&c).unwrap_err().contains("producer"));
    let c = chain(vec![step(
        "loop",
        Action::Repeat {
            max_iterations: 2,
            until: Condition::Exists {
                value: output("producer", ""),
            },
            steps: vec![step(
                "parallel",
                Action::Parallel {
                    branches: vec![
                        vec![delay("producer")],
                        vec![step(
                            "consumer",
                            Action::If {
                                condition: Condition::Exists {
                                    value: output("producer", ""),
                                },
                                then_steps: vec![],
                                else_steps: vec![],
                            },
                        )],
                    ],
                },
            )],
        },
    )]);
    assert!(validate(&c).unwrap_err().contains("producer"));
    let c = chain(vec![step(
        "loop",
        Action::Repeat {
            max_iterations: 2,
            until: Condition::Exists {
                value: output("producer", ""),
            },
            steps: vec![delay("producer")],
        },
    )]);
    validate(&c).unwrap();
}

#[test]
fn workspace_prompt_references_validate_names_and_pointers() {
    for (prompt, reason) in [
        ("{{output:missing}}", "Unknown output"),
        ("{{output:producer#text}}", "pointers"),
    ] {
        let c = chain(vec![
            delay("producer"),
            step(
                "consumer",
                Action::Workspace {
                    prompt: prompt.into(),
                    model: None,
                    effort: None,
                },
            ),
        ]);
        assert!(validate(&c).unwrap_err().contains(reason));
    }
}

fn failed_attempt(s: &Store, status: &str) -> TaskRun {
    let mut run = s
        .create(
            "retry-fixture",
            chain(vec![delay("action")]),
            Scope::default(),
            json!({}),
            0,
            true,
        )
        .unwrap();
    run.state = "attention".into();
    run.progress.actions = 1;
    run.progress.receipts.insert(
        "action".into(),
        Receipt {
            sequence: 1,
            address: "action".into(),
            step_id: "action".into(),
            label: "Prior action".into(),
            state: status.into(),
            operation: store::id(),
            started_at: 10,
            finished_at: Some(20),
            wake_at: None,
            output: Some(json!({"preview":"Original"})),
            error: Some("Interrupted action".into()),
            child: Some(json!({"runId":"original-child"})),
        },
    );
    s.save(&mut run, "attention", "Interrupted action").unwrap();
    run
}

#[test]
fn retry_retains_attempts_and_operation_scoped_outputs_after_reload() {
    for status in ["failed", "unknown"] {
        let temp = tempfile::tempdir().unwrap();
        let s = Store::open(temp.path()).unwrap();
        let mut run = failed_attempt(&s, status);
        let original = run.progress.receipts["action"].clone();
        let mut stale = run.clone();
        let old_result = json!({"text":"Full original result"});
        let original_path = s
            .root
            .join("actions")
            .join(&original.operation)
            .join("result.json");
        if status == "failed" {
            adapters::atomic_json(&original_path, &old_result).unwrap();
        }
        s.retry(&mut run).unwrap();
        assert!(s.retry(&mut stale).is_err());
        assert_eq!(run.progress.actions, 1);
        assert!(run.progress.receipts.is_empty());
        assert_eq!(run.state, "queued");
        // An uncertain result can arrive after the retry and still be inspected.
        if status == "unknown" {
            adapters::atomic_json(&original_path, &old_result).unwrap();
        }
        let mut replacement = original.clone();
        replacement.sequence = 2;
        replacement.operation = store::id();
        replacement.state = "completed".into();
        replacement.error = None;
        replacement.output = Some(json!({"text":"Replacement result"}));
        run.progress
            .receipts
            .insert("action".into(), replacement.clone());
        run.progress.actions = 2;
        run.state = "finished".into();
        s.save(&mut run, "finished", "Replacement completed")
            .unwrap();
        let reopened = Store::open(temp.path()).unwrap();
        let events = reopened.events(&run.id, 0).unwrap();
        let attempts: Vec<_> = events.iter().flat_map(|e| &e.attempts).collect();
        assert_eq!(attempts.len(), 1);
        assert_eq!(
            serde_json::to_value(attempts[0]).unwrap(),
            serde_json::to_value(&original).unwrap()
        );
        assert_eq!(
            reopened
                .step_output(&run.id, "action", Some(&original.operation))
                .unwrap(),
            old_result
        );
        assert_eq!(
            reopened.step_output(&run.id, "action", None).unwrap(),
            replacement.output.clone().unwrap()
        );
        assert_eq!(
            reopened
                .step_receipt(&run.id, "action", Some(&replacement.operation))
                .unwrap()
                .sequence,
            2
        );
        assert!(reopened
            .step_output(&run.id, "wrong-address", Some(&original.operation))
            .is_err());
        let other = reopened
            .create(
                "other-task",
                chain(vec![delay("action")]),
                Scope::default(),
                json!({}),
                0,
                true,
            )
            .unwrap();
        assert!(reopened
            .step_output(&other.id, "action", Some(&original.operation))
            .is_err());
    }
}

#[test]
fn retry_rolls_back_receipt_removal_when_journal_write_fails() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = failed_attempt(&s, "unknown");
    let before = serde_json::to_value(&run).unwrap();
    s.connection().unwrap().execute_batch("CREATE TRIGGER fail_retry BEFORE INSERT ON events WHEN NEW.kind='retry' BEGIN SELECT RAISE(ABORT,'injected journal failure'); END;").unwrap();
    assert!(s
        .retry(&mut run)
        .unwrap_err()
        .contains("injected journal failure"));
    assert_eq!(serde_json::to_value(&run).unwrap(), before);
    assert_eq!(
        serde_json::to_value(s.get(&run.id).unwrap()).unwrap(),
        before
    );
    assert!(!s
        .events(&run.id, 0)
        .unwrap()
        .iter()
        .any(|e| e.kind == "retry"));
}

#[test]
fn legacy_retry_events_remain_readable() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut run = failed_attempt(&s, "failed");
    s.save(&mut run, "retry", "Task updated").unwrap();
    let events = s.events(&run.id, 0).unwrap();
    assert_eq!(events.last().unwrap().detail, "Task updated");
    assert!(events.last().unwrap().attempts.is_empty());
    assert!(s
        .step_output(&run.id, "action", Some("missing-operation"))
        .is_err());
}
#[test]
fn input_waits_do_not_poll_and_future_timers_are_not_ready() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "wait",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now() + 600,
            true,
        )
        .unwrap();
    assert!(s.ready_ids(now()).unwrap().is_empty());
    r.state = "waiting".into();
    r.due_at = None;
    s.save(&mut r, "wait", "Input").unwrap();
    assert!(s.ready_ids(now()).unwrap().is_empty());
    assert_eq!(s.next_due().unwrap(), Some(r.deadline_at));
}
#[test]
fn paused_active_actions_are_recovered_too() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "pause",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now(),
            true,
        )
        .unwrap();
    r.state = "paused".into();
    r.progress.receipts.insert(
        "a".into(),
        Receipt {
            sequence: 0,
            address: "a".into(),
            step_id: "a".into(),
            label: "a".into(),
            state: "running".into(),
            operation: store::id(),
            started_at: now(),
            finished_at: None,
            wake_at: None,
            output: None,
            error: None,
            child: None,
        },
    );
    s.save(&mut r, "pause", "Paused").unwrap();
    s.recover().unwrap();
    assert_eq!(s.get(&r.id).unwrap().state, "attention");
}
#[test]
fn schedule_activation_and_draft_consumption_are_atomic() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let mut r = s
        .create(
            "schedule",
            chain(vec![delay("a")]),
            Scope::default(),
            json!({}),
            now(),
            false,
        )
        .unwrap();
    let mut stale = r.clone();
    let trigger = Trigger::Interval {
        seconds: 60,
        anchor: now(),
    };
    s.activate_schedule(&mut r, trigger.clone()).unwrap();
    assert!(s.activate_schedule(&mut stale, trigger).is_err());
    assert_eq!(s.schedules().unwrap().len(), 1);
    assert_eq!(s.get(&r.id).unwrap().state, "finished");
}
#[test]
fn coalescing_calendar_after_years_chooses_a_recent_occurrence() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let trigger = Trigger::Calendar {
        timezone: "UTC".into(),
        hour: 9,
        minute: 0,
        weekdays: vec![0, 1, 2, 3, 4, 5, 6],
    };
    s.save_schedule(
        Schedule {
            id: "calendar".into(),
            revision: 0,
            name: "Long sleep".into(),
            enabled: true,
            chain: chain(vec![delay("a")]),
            scope: Scope::default(),
            inputs: json!({}),
            next_due_at: trigger.next(0).unwrap(),
            trigger,
            created_at: 0,
            tzdb_version: chrono_tz::IANA_TZDB_VERSION.into(),
        },
        None,
    )
    .unwrap();
    s.fire_schedules(now()).unwrap();
    let rows = s.list("active", None, 0).unwrap();
    let r = s.get(&rows[0].id).unwrap();
    assert!(now() - r.occurrence.unwrap() < 86400);
}
#[test]
fn text_artifacts_are_immutable_and_hash_checked() {
    let temp = tempfile::tempdir().unwrap();
    let s = Store::open(temp.path()).unwrap();
    let a = adapters::snapshot(&s, &Scope::default(), json!("Original paper"), "paper.md").unwrap();
    let b = adapters::snapshot(&s, &Scope::default(), json!("Revised paper"), "paper.md").unwrap();
    assert_ne!(a["hash"], b["hash"]);
    assert!(adapters::artifact_path(&s, &a).is_ok());
    std::fs::write(a["path"].as_str().unwrap(), "Changed externally").unwrap();
    assert!(adapters::artifact_path(&s, &a).is_err());
    assert!(adapters::snapshot(&s, &Scope::default(), json!("bad"), "../escape").is_err());
}
#[test]
fn file_and_tex_tree_snapshots_are_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("paper");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("main.tex"), "\\input{chapter}").unwrap();
    std::fs::write(source.join("chapter.tex"), "Research").unwrap();
    let s = Store::open(&temp.path().join("tasks")).unwrap();
    let scope = Scope {
        runtime_root: Some(source.to_string_lossy().into()),
        root_identity: Some(crate::workbench::store::root_identity(&source).unwrap()),
        ..Scope::default()
    };
    let artifact = adapters::snapshot(&s, &scope, json!({"path":source}), "paper").unwrap();
    assert_eq!(artifact["kind"], "artifactTree");
    assert_eq!(artifact["manifest"].as_array().unwrap().len(), 2);
    assert!(adapters::artifact_path(&s, &artifact).is_ok());
    let relative_tree = adapters::snapshot(&s, &scope, json!({"path":"."}), "paper").unwrap();
    assert_eq!(relative_tree["hash"], artifact["hash"]);
    let relative_file =
        adapters::snapshot(&s, &scope, json!({"path":"chapter.tex"}), "chapter.tex").unwrap();
    assert_eq!(relative_file["kind"], "artifact");
    assert_eq!(
        std::fs::read_to_string(adapters::artifact_path(&s, &relative_file).unwrap()).unwrap(),
        "Research"
    );
    let outside = temp.path().join("outside.md");
    std::fs::write(&outside, "Outside").unwrap();
    assert!(adapters::snapshot(&s, &scope, json!({"path":outside}), "paper.md").is_err());
}
#[test]
fn foreach_binds_each_input_once() {
    let c = chain(vec![step(
        "items",
        Action::ForEach {
            input: Binding::Input {
                key: "papers".into(),
            },
            max_items: 2,
            steps: vec![delay("work")],
        },
    )]);
    let mut p = state::Progress::default();
    for (i, item) in ["a", "b"].iter().enumerate() {
        let Advance::Ready(ready) = state::advance(&c, &mut p, &json!({"papers":["a","b"]})) else {
            panic!()
        };
        assert_eq!(ready[0].inputs["item"], *item);
        assert_eq!(ready[0].inputs["index"], i);
        done(&mut p, &ready[0], json!({}));
    }
    assert!(matches!(
        state::advance(&c, &mut p, &json!({"papers":["changed"]})),
        Advance::Finished
    ));
}

mod coordinator;
