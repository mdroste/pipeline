use super::*;

#[test]
fn followup_queue_is_durable_scoped_reorderable_and_does_not_send() {
    let f = fixture(false);
    let a = followups::enqueue(&f.store, &f.session, "First question", None, None, "a").unwrap();
    followups::enqueue(&f.store, &f.session, "Second question", None, None, "b").unwrap();
    let rows = followups::list(&f.store, &f.session).unwrap();
    let second = &rows[1];
    followups::control(
        &f.store,
        &f.session,
        second["id"].as_str().unwrap(),
        1,
        "up",
    )
    .unwrap();
    let rows = followups::list(&f.store, &f.session).unwrap();
    assert_eq!(rows[0]["request"]["text"], "Second question");
    assert!(followups::control(
        &f.store,
        &f.session,
        a[0]["id"].as_str().unwrap(),
        1,
        "cancel"
    )
    .is_err());
    assert!(f
        .store
        .conversation_snapshot(&f.session)
        .unwrap()
        .turns
        .is_empty());
    assert!(
        followups::enqueue(&f.store, &f.session, "Different content", None, None, "a").is_err()
    );
}
#[test]
fn followup_unknown_acknowledgement_cannot_be_dispatched_again() {
    let f = fixture(false);
    let rows = followups::enqueue(&f.store, &f.session, "One request", None, None, "a").unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let fp = rows[0]["fingerprint"].as_str().unwrap();
    followups::prepare_dispatch(&f.store, &f.session, id, fp).unwrap();
    let rows = followups::reconcile(&f.store, &f.session, id, None).unwrap();
    assert_eq!(rows[0]["state"], "attention");
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, fp).is_err());
}
#[test]
fn local_monitor_deduplicates_backoff_and_coalesces_missed_checks() {
    let f = fixture(true);
    fs::write(f.root.join("paper.md"), "v1").unwrap();
    let m = monitors::Monitor {
        kind: "accepted_file".into(),
        target: "paper.md".into(),
        interval_seconds: 60,
        notify: true,
        network_consent: false,
    };
    let r = monitors::save(&f.store, &f.ws, "Paper changes", m.clone(), "monitor").unwrap();
    let v1 = monitors::local(&f.store, &f.ws, &m).unwrap();
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 1, Ok(v1.clone()), 1000).unwrap());
    fs::write(f.root.join("paper.md"), "v2").unwrap();
    let v2 = monitors::local(&f.store, &f.ws, &m).unwrap();
    assert!(monitors::record_outcome(&f.store, &f.ws, &r.id, 2, Ok(v2.clone()), 2000).unwrap());
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 3, Ok(v2), 3000).unwrap());
    assert!(monitors::record_outcome(&f.store, &f.ws, &r.id, 4, Ok(v1), 4000).unwrap()); // returning to the initial baseline is still a distinct changed state
    assert_eq!(
        monitors::list(&f.store, &f.ws).unwrap()["checks"][0]["nextDueAt"],
        4060
    );
    for (revision, t) in [(5, 5000), (6, 6000), (7, 7000)] {
        monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Err(WorkbenchError::invalid("Network unavailable")),
            t,
        )
        .unwrap();
    }
    let list = monitors::list(&f.store, &f.ws).unwrap();
    assert_eq!(list["checks"][0]["nextDueAt"], 7480);
    assert_eq!(monitors::due(&f.store, 100000).unwrap().len(), 1);
}
#[test]
fn monitor_repeated_transitions_and_failure_episodes_have_distinct_occurrences() {
    let f = fixture(true);
    fs::write(f.root.join("paper.md"), "v1").unwrap();
    let r = monitors::save(
        &f.store,
        &f.ws,
        "Paper changes",
        monitors::Monitor {
            kind: "accepted_file".into(),
            target: "paper.md".into(),
            interval_seconds: 60,
            notify: true,
            network_consent: false,
        },
        "monitor",
    )
    .unwrap();
    for (revision, value, notify) in [
        (1, "A", false),
        (2, "B", true),
        (3, "A", true),
        (4, "B", true),
        (5, "B", false),
    ] {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Ok(json!(value)),
                revision * 1000
            )
            .unwrap(),
            notify
        );
        // Retrying even with a different completion time must not consume another occurrence.
        assert!(!monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Ok(json!(value)),
            revision * 1000 + 1
        )
        .unwrap());
        if revision == 2 {
            let attention = monitors::list(&f.store, &f.ws).unwrap();
            monitors::control(
                &f.store,
                &f.ws,
                attention["attention"][0]["id"].as_str().unwrap(),
                0,
                "acknowledge",
            )
            .unwrap();
        }
    }
    for revision in 6..=8 {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Err(WorkbenchError::invalid("Unavailable")),
                revision * 1000
            )
            .unwrap(),
            revision == 8
        );
        assert!(!monitors::record_outcome(
            &f.store,
            &f.ws,
            &r.id,
            revision,
            Err(WorkbenchError::invalid("Unavailable")),
            revision * 1000 + 1
        )
        .unwrap());
    }
    assert!(!monitors::record_outcome(&f.store, &f.ws, &r.id, 9, Ok(json!("B")), 9000).unwrap());
    for revision in 10..=12 {
        assert_eq!(
            monitors::record_outcome(
                &f.store,
                &f.ws,
                &r.id,
                revision,
                Err(WorkbenchError::invalid("Unavailable")),
                revision * 1000
            )
            .unwrap(),
            revision == 12
        );
    }
    let list = monitors::list(&f.store, &f.ws).unwrap();
    assert_eq!(list["attention"].as_array().unwrap().len(), 5);
    assert_eq!(list["checks"][0]["revision"], 13);
    let (_, captured_revision) = &monitors::due(&f.store, 100000).unwrap()[0];
    assert_eq!(*captured_revision, 13);
    monitors::control(&f.store, &f.ws, &r.id, 13, "pause").unwrap();
    monitors::control(&f.store, &f.ws, &r.id, 14, "resume").unwrap();
    assert!(!monitors::record_outcome(
        &f.store,
        &f.ws,
        &r.id,
        *captured_revision,
        Ok(json!("C")),
        100000
    )
    .unwrap());
    assert_eq!(
        monitors::list(&f.store, &f.ws).unwrap()["checks"][0]["revision"],
        15
    );
}
#[test]
fn remote_monitor_requires_both_explicit_consents() {
    let f = fixture(false);
    let m = monitors::Monitor {
        kind: "metadata_query".into(),
        target: "macroeconomic mechanisms".into(),
        interval_seconds: 3600,
        notify: true,
        network_consent: true,
    };
    assert!(monitors::save(&f.store, &f.ws, "Query", m.clone(), "monitor").is_err());
    crate::workbench::acquisition::set_network(&f.store, &f.ws, true).unwrap();
    assert!(monitors::save(&f.store, &f.ws, "Query", m, "monitor").is_ok());
}

#[test]
fn followup_history_pages_never_hide_the_active_queue() {
    let f = fixture(false);
    for i in 0..105 {
        let rows = followups::enqueue(
            &f.store,
            &f.session,
            &format!("History {i}"),
            None,
            None,
            &format!("history-{i}"),
        )
        .unwrap();
        let row = &rows[0];
        followups::control(
            &f.store,
            &f.session,
            row["id"].as_str().unwrap(),
            row["revision"].as_i64().unwrap(),
            "cancel",
        )
        .unwrap();
    }
    let rows =
        followups::enqueue(&f.store, &f.session, "Still visible", None, None, "pending").unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 101);
    assert_eq!(rows[0]["request"]["text"], "Still visible");
    assert_eq!(rows[1]["request"]["text"], "History 104");
    let older = followups::list_history(&f.store, &f.session, 100).unwrap();
    assert_eq!(older.as_array().unwrap().len(), 6);
    assert_eq!(older[0]["request"]["text"], "Still visible");
    assert_eq!(older[5]["request"]["text"], "History 0");
    followups::control(
        &f.store,
        &f.session,
        older[0]["id"].as_str().unwrap(),
        older[0]["revision"].as_i64().unwrap(),
        "cancel",
    )
    .unwrap();
    let next =
        followups::enqueue(&f.store, &f.session, "Runnable", None, None, "runnable").unwrap();
    let request = followups::prepare_dispatch(
        &f.store,
        &f.session,
        next[0]["id"].as_str().unwrap(),
        next[0]["fingerprint"].as_str().unwrap(),
    )
    .unwrap()
    .0;
    assert_eq!(request.text, "Runnable");
}

#[test]
fn followup_refresh_reviews_new_turns_and_preserves_the_saved_request() {
    let f = fixture(false);
    let first =
        followups::enqueue(&f.store, &f.session, "First", None, None, "first").unwrap()[0].clone();
    let second = followups::enqueue(
        &f.store,
        &f.session,
        "Keep this wording",
        Some("saved-model".into()),
        Some("high".into()),
        "second",
    )
    .unwrap()[1]
        .clone();
    let id = second["id"].as_str().unwrap();
    let old_fingerprint = second["fingerprint"].as_str().unwrap();
    let (sent, _) = followups::prepare_dispatch(
        &f.store,
        &f.session,
        first["id"].as_str().unwrap(),
        first["fingerprint"].as_str().unwrap(),
    )
    .unwrap();
    let c = f.store.connection().unwrap();
    c.execute("INSERT INTO session_bindings(id,session_id,runtime_namespace,provider_thread_id,incarnation,created_at) VALUES('b',?1,'test','thread',1,'t')", [&f.session]).unwrap();
    c.execute("INSERT INTO turns(id,binding_id,client_submission_id,state,created_at,updated_at,terminal_at) VALUES('first-turn','b',?1,'completed','t','t','t')", [sent.client_submission_id]).unwrap();
    followups::reconcile(&f.store, &f.session, first["id"].as_str().unwrap(), None).unwrap();
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, old_fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    assert!(review.conversation_changed);
    assert_eq!(review.request.text, "Keep this wording");
    assert_eq!(review.request.model.as_deref(), Some("saved-model"));
    assert_eq!(review.request.effort.as_deref(), Some("high"));
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, "unreviewed").is_err());
    // A selection change after preview must invalidate the confirmation.
    desk::save_context(&f.store, &f.session, 0, vec![]).unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    research::save_config(
        &f.store,
        research::SaveWorkspaceConfigRequest {
            workspace_id: Some(f.ws.clone()),
            expected_revision: Some(0),
            body: json!({"contextBudgetBytes":16384,"commandNetwork":true}),
            operation_id: "changed-settings".into(),
        },
    )
    .unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    assert!(review.settings_changed);
    let updated =
        followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).unwrap();
    let row = updated
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap();
    assert_eq!(row["state"], "queued");
    assert_eq!(row["revision"], 2);
    assert!(followups::prepare_dispatch(&f.store, &f.session, id, old_fingerprint).is_err());
    let (request, _) = followups::prepare_dispatch(
        &f.store,
        &f.session,
        id,
        row["fingerprint"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(request.text, "Keep this wording");
    assert_eq!(request.model.as_deref(), Some("saved-model"));
    assert!(followups::prepare_dispatch(
        &f.store,
        &f.session,
        id,
        row["fingerprint"].as_str().unwrap()
    )
    .is_err());
    assert!(followups::review_context(&f.store, &f.session, id, 2).is_err());
}

#[test]
fn followup_refresh_rejects_changed_roots_and_stale_queue_revisions() {
    let f = fixture(true);
    let rows = followups::enqueue(&f.store, &f.session, "Question", None, None, "q").unwrap();
    let id = rows[0]["id"].as_str().unwrap();
    let review = followups::review_context(&f.store, &f.session, id, 1).unwrap();
    fs::rename(&f.root, f.root.with_file_name("old-project")).unwrap();
    fs::create_dir(&f.root).unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
    assert!(followups::review_context(&f.store, &f.session, id, 1).is_err());
    followups::control(&f.store, &f.session, id, 1, "cancel").unwrap();
    assert!(followups::refresh_context(&f.store, &f.session, id, 1, &review.fingerprint).is_err());
}
