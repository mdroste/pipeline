use super::*;

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
    let c: Chain = serde_json::from_str(include_str!("../fixtures/review-chain.json")).unwrap();
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

#[test]
fn reserved_step_ids_are_rejected_before_they_can_shadow_decisions() {
    let gate = |inner: Step| {
        chain(vec![
            step(
                "gate",
                Action::If {
                    condition: Condition::Equals {
                        left: Binding::Input {
                            key: "answer".into(),
                        },
                        right: json!(true),
                    },
                    then_steps: vec![inner, delay("b")],
                    else_steps: vec![],
                },
            ),
            delay("c"),
        ])
    };
    let nested_done = step(
        "done",
        Action::If {
            condition: Condition::Equals {
                left: Binding::Input {
                    key: "answer".into(),
                },
                right: json!(true),
            },
            then_steps: vec![delay("a")],
            else_steps: vec![],
        },
    );
    assert!(definition::validate(&gate(nested_done)).is_err());
    assert!(definition::validate(&gate(delay("enter"))).is_err());
    assert!(definition::validate(&gate(delay("0"))).is_err());
    assert_eq!(definition::validate(&gate(delay("inner"))), Ok(()));
}
