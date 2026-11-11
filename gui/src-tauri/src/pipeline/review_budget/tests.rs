use super::*;

#[tokio::test]
async fn warmups_and_internal_retries_reserve_additional_calls() {
    let run = crate::run_scope::ActiveScope::begin("budget-internal-attempts");
    crate::run_scope::scoped(run.scope().clone(), async {
        let settings = Settings {
            review_max_calls: 2,
            ..Default::default()
        };
        let _budget = begin(&settings, Instant::now());
        let permit = admit().unwrap();
        let (result, usage) = permit
            .scope(super::super::logging::measure_usage(async {
                super::super::logging::record_provider_attempt()?;
                super::super::logging::record_provider_attempt()?;
                super::super::logging::record_provider_attempt()
            }))
            .await;
        assert!(stopped(&result.unwrap_err()));
        assert_eq!(usage.provider_attempts, 2);
        assert!(
            admit().is_err(),
            "an internal retry consumes reserved capacity too"
        );
        permit.complete("local", "fixture", usage, &settings);
        let state = current().unwrap();
        let state = lock(&state);
        assert_eq!(state.in_flight, 0);
        assert_eq!(state.completed_calls, 2);
    })
    .await;
}

#[tokio::test]
async fn dropping_an_invocation_releases_all_of_its_internal_reservations() {
    let run = crate::run_scope::ActiveScope::begin("budget-drop-internal-attempts");
    crate::run_scope::scoped(run.scope().clone(), async {
        let settings = Settings {
            review_max_calls: 3,
            ..Default::default()
        };
        let _budget = begin(&settings, Instant::now());
        let permit = admit().unwrap();
        permit
            .scope(async {
                super::super::logging::record_provider_attempt().unwrap();
                super::super::logging::record_provider_attempt().unwrap();
            })
            .await;
        drop(permit);
        assert_eq!(lock(&current().unwrap()).in_flight, 0);
        // Recorded attempts stay spent even if the enclosing future is dropped.
        let final_call = admit().unwrap();
        assert!(
            admit().is_err(),
            "dropped attempts plus this reservation reach the cap"
        );
        final_call
            .scope(async {
                super::super::logging::record_provider_attempt().unwrap();
            })
            .await;
        assert!(admit().is_err());
    })
    .await;
}

#[tokio::test]
async fn preparation_calls_reserve_capacity_and_keep_finished_spend() {
    let run = crate::run_scope::ActiveScope::begin("budget-preparation");
    crate::run_scope::scoped(run.scope().clone(), async {
        let settings = Settings {
            review_max_calls: 2,
            claude_access_mode: "api".into(),
            ..Default::default()
        };
        let _budget = begin(&settings, Instant::now());
        let first = admit().unwrap();
        let second = crate::run_scope::spawn(async { admit() })
            .await
            .unwrap()
            .unwrap();
        assert!(
            admit().is_err(),
            "concurrent helpers must reserve their calls"
        );
        first.complete(
            "claude",
            "claude-sonnet-test",
            CallUsage {
                provider_attempts: 1,
                input_tokens: 100_000,
                output_tokens: 50_000,
                ..Default::default()
            },
            &settings,
        );
        second.complete(
            "claude",
            "claude-sonnet-test",
            CallUsage {
                provider_attempts: 1,
                ..Default::default()
            },
            &settings,
        );
        assert!(check().unwrap_err().contains("limit of 2 model calls"));
        assert!((lock(&current().unwrap()).cost - 1.05).abs() < 1e-9);
    })
    .await;
}

#[tokio::test]
async fn extraction_cost_and_preparation_time_stop_orientation_and_comparison() {
    let run = crate::run_scope::ActiveScope::begin("budget-preparation-cost");
    crate::run_scope::scoped(run.scope().clone(), async {
        let settings = Settings {
            review_max_cost_usd: 1.0,
            claude_access_mode: "api".into(),
            ..Default::default()
        };
        {
            let _budget = begin(&settings, Instant::now());
            admit().unwrap().complete(
                "claude",
                "claude-sonnet-test",
                CallUsage {
                    provider_attempts: 1,
                    input_tokens: 100_000,
                    output_tokens: 50_000,
                    ..Default::default()
                },
                &settings,
            );
            let bus: crate::emit::EventBus = Arc::new(crate::emit::NullEvents);
            for stage in ["orientation", "reconciliation", "extraction-pages-3-4"] {
                let mut request =
                    super::super::call::OwnedRequest::new(&bus, stage, stage, "fixture", 30);
                request.settings = Arc::new(settings.clone());
                assert!(stopped(
                    &super::super::call::execute_text(request).await.unwrap_err()
                ));
            }
        }
        let settings = Settings {
            review_max_minutes: 1,
            ..Default::default()
        };
        let _budget = begin(
            &settings,
            Instant::now() - std::time::Duration::from_secs(61),
        );
        assert!(check().unwrap_err().contains("time limit"));
        assert!(admit().is_err());
    })
    .await;
}

#[tokio::test]
async fn dropped_reservations_and_other_runs_do_not_consume_this_budget() {
    let a = crate::run_scope::ActiveScope::begin("budget-isolated-a");
    let b = crate::run_scope::ActiveScope::begin("budget-isolated-b");
    crate::run_scope::scoped(a.scope().clone(), async {
        let settings = Settings {
            review_max_calls: 1,
            ..Default::default()
        };
        let _budget = begin(&settings, Instant::now());
        let permit = admit().unwrap();
        assert!(admit().is_err());
        crate::run_scope::scoped(b.scope().clone(), async {
            assert!(admit().is_ok());
        })
        .await;
        drop(permit);
        assert!(admit().is_ok());
    })
    .await;
    crate::run_scope::scoped(a.scope().clone(), async {
        assert!(!active());
    })
    .await;
}
