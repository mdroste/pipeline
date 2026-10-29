use super::*;

#[test]
fn strict_chain_roundtrip() {
    let c = chain(vec![delay("a")]);
    validate(&c).unwrap();
    let json = serde_json::to_string(&c).unwrap();
    let decoded: Chain = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, c);
}

#[test]
fn limits_match_the_builder_ranges() {
    // The automation builder offers these same maximums.
    let mut c = chain(vec![delay("a")]);
    c.limits.max_actions = 1024;
    c.limits.deadline_hours = 8760;
    validate(&c).unwrap();
    c.limits.max_actions = 1025;
    assert!(validate(&c).is_err());
    c.limits.max_actions = 1024;
    c.limits.deadline_hours = 8761;
    assert!(validate(&c).is_err());
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
