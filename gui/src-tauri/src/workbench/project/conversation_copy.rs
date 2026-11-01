//! Interactive Edit turns acquire a review copy without creating another chat.
//! The turn permit and project lock cover preparation. Existing manual copies
//! and automation scopes are never redirected. Acceptance/undo remain in tasks.
//! A settled copy is reclaimed here, when the next turn moves off it, rather
//! than at acceptance: until then the conversation can undo back onto it.
use super::*;

pub(crate) fn prepare(store: &Store, session_id: &str) -> WorkbenchResult<()> {
    if crate::workbench::discovery::is_role(store, session_id)? {
        return Ok(());
    }
    let snapshot = store.session_snapshot(session_id)?;
    let Some(workspace) = snapshot.workspace.filter(|w| w.root.is_some()) else {
        return Ok(());
    };
    let mut session = snapshot.session;
    let mut mode = "inspect".to_owned();
    let mut file_mode = "review".to_owned();
    for values in [
        research::load_config(store, None)?.body,
        research::load_config(store, Some(&workspace.id))?.body,
        session.overrides.clone(),
    ] {
        if let Some(value) = values["mode"].as_str() {
            mode = value.into();
        }
        if let Some(value) = values["fileMode"].as_str() {
            file_mode = value.into();
        }
    }
    let automatic = session.overrides["automaticWorkingCopy"].as_bool() == Some(true);
    let existing = session.overrides["projectCheckpointId"]
        .as_str()
        .map(str::to_owned);
    if existing.is_some() && !automatic {
        return Ok(());
    }
    let _guard = lock(store, &workspace.id)?;
    let mut replaced = None;
    if let Some(cp) = existing {
        let checkpoint: Checkpoint = decode(&record(store, &workspace.id, &cp, "checkpoint")?)?;
        if ["isolated", "review"].contains(&checkpoint.state.as_str()) {
            session_task_root(store, &workspace.id, &cp)?;
            if file_mode == "direct" {
                return Err(WorkbenchError::conflict("Accept or discard this conversation's changes in Review edits before switching to direct edits."));
            }
            return Ok(());
        }
        session
            .overrides
            .as_object_mut()
            .ok_or_else(|| WorkbenchError::invalid("Invalid conversation settings"))?
            .remove("projectCheckpointId");
        session
            .overrides
            .as_object_mut()
            .unwrap()
            .remove("automaticWorkingCopy");
        replaced = Some(cp);
    }
    if mode == "edit" && file_mode == "review" {
        let source = store.registered_root(&workspace)?;
        let settings: ProjectHomeSettings = decode(&home_record(store, &workspace.id)?)?;
        let inventory = inventory_at(&source, &settings.ignored_paths)?;
        if !inventory.complete {
            return Err(WorkbenchError::invalid("This project is too large or contains files that cannot be copied safely. Select files in Review edits, exclude large data folders in Project settings, or choose Direct edits under File changes in Assistant settings. Each copied file must fit within 32 MB and the copy within 256 MB."));
        }
        let task = tasks::create_task(
            store,
            &workspace.id,
            format!("Conversation: {}", session.title),
            None,
            vec![],
            vec![],
        )?;
        let cp = tasks::checkpoint(
            store,
            &workspace.id,
            &task.id,
            inventory.files.into_iter().map(|f| f.path).collect(),
            "copy",
        )?;
        session.overrides["projectCheckpointId"] = json!(cp.id);
        session.overrides["automaticWorkingCopy"] = json!(true);
    }
    if session.overrides != store.session_snapshot(session_id)?.session.overrides {
        store.update_session(super::super::store::UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: id("conversation-copy")?,
            overrides: Some(session.overrides),
            title: None,
            draft: None,
            archived: None,
            preset_id: None,
            paper_id: None,
            clear_paper: None,
        })?;
    }
    if let Some(cp) = replaced {
        // Only disk space is at stake, so a copy that can't go yet never stops
        // the turn. It stays where the researcher can still find it.
        let _ = tasks::reclaim_replaced_copy(store, &workspace.id, &cp);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;
    fn session(f: &Fixture, mode: &str) -> String {
        let session = f
            .store
            .create_session(crate::workbench::store::CreateSessionRequest {
                workspace_id: Some(f.ws.clone()),
                title: "Paper chat".into(),
                operation_id: id("session").unwrap(),
            })
            .unwrap()
            .record;
        f.store
            .update_session(crate::workbench::store::UpdateSessionRequest {
                session_id: session.id.clone(),
                expected_revision: session.revision,
                operation_id: id("edit").unwrap(),
                overrides: Some(json!({"mode":mode,"runtime":"openai-api","model":"fixture"})),
                draft: Some("Keep my draft".into()),
                title: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        session.id
    }
    #[test]
    fn ordinary_chat_reuses_copy_and_acceptance_and_undo_preserve_originals() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        prepare(&f.store, &session).unwrap();
        let snapshot = f.store.session_snapshot(&session).unwrap();
        assert_eq!(snapshot.session.draft, "Keep my draft");
        assert_eq!(snapshot.session.overrides["model"], "fixture");
        let cp = snapshot.session.overrides["projectCheckpointId"]
            .as_str()
            .unwrap();
        let root = f.store.runtime_root(&session).unwrap();
        fs::write(root.join("paper.md"), "revised").unwrap();
        fs::write(root.join("new.md"), "new").unwrap();
        prepare(&f.store, &session).unwrap();
        assert_eq!(f.store.runtime_root(&session).unwrap(), root);
        assistant_files::capture_session(&f.store, &session).unwrap();
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "original"
        );
        let captured = record(&f.store, &f.ws, cp, "checkpoint").unwrap();
        let app = tasks::apply(
            &f.store,
            &f.ws,
            cp,
            vec!["paper.md".into(), "new.md".into()],
            captured.revision,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "revised"
        );
        tasks::undo(&f.store, &f.ws, &app.id).unwrap();
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "original"
        );
        assert!(!f.root.join("new.md").exists());
    }
    #[test]
    fn inspect_stays_in_project_and_accepted_copy_is_replaced_next_turn() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let inspect = session(&f, "inspect");
        prepare(&f.store, &inspect).unwrap();
        assert_eq!(
            f.store.runtime_root(&inspect).unwrap(),
            f.root.canonicalize().unwrap()
        );
        let edit = session(&f, "edit");
        prepare(&f.store, &edit).unwrap();
        let root = f.store.runtime_root(&edit).unwrap();
        let cp = f.store.session_snapshot(&edit).unwrap().session.overrides["projectCheckpointId"]
            .as_str()
            .unwrap()
            .to_owned();
        tasks::reject(&f.store, &f.ws, &cp).unwrap();
        prepare(&f.store, &edit).unwrap();
        assert_ne!(f.store.runtime_root(&edit).unwrap(), root);
    }
    fn working_copy(f: &Fixture, session: &str) -> (String, PathBuf) {
        prepare(&f.store, session).unwrap();
        let cp = f.store.session_snapshot(session).unwrap().session.overrides
            ["projectCheckpointId"]
            .as_str()
            .unwrap()
            .to_owned();
        (cp, f.store.runtime_root(session).unwrap())
    }
    #[test]
    fn the_next_turn_removes_a_settled_copy_and_undo_still_works() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        let (cp, root) = working_copy(&f, &session);
        fs::write(root.join("paper.md"), "revised").unwrap();
        assistant_files::capture_session(&f.store, &session).unwrap();
        let captured = record(&f.store, &f.ws, &cp, "checkpoint").unwrap();
        let app = tasks::apply(
            &f.store,
            &f.ws,
            &cp,
            vec!["paper.md".into()],
            captured.revision,
        )
        .unwrap();
        assert!(root.exists(), "kept until the conversation moves on");
        let (next, next_root) = working_copy(&f, &session);
        assert_ne!(next, cp);
        assert!(!root.exists());
        assert!(next_root.exists());
        tasks::undo(&f.store, &f.ws, &app.id).unwrap();
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "original"
        );
        // With no folder left, Undo finishes the edit instead of reopening it.
        let undone = record(&f.store, &f.ws, &cp, "checkpoint").unwrap();
        assert_eq!(undone.body["state"], "undone");
        assert!(session_task_root(&f.store, &f.ws, &cp).is_err());
        assert!(tasks::apply(
            &f.store,
            &f.ws,
            &cp,
            vec!["paper.md".into()],
            undone.revision
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "original"
        );
    }
    #[test]
    fn a_reviewable_edit_whose_copy_vanished_says_so_plainly() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        let (cp, root) = working_copy(&f, &session);
        fs::remove_dir_all(&root).unwrap();
        let error = session_task_root(&f.store, &f.ws, &cp).unwrap_err();
        assert!(error.message.contains("Task copy is missing"), "{error:?}");
    }
    #[test]
    fn files_in_build_and_cache_folders_keep_their_copy() {
        for unseen in [
            "output/target/estimates.csv",
            "table1.out",
            ".cache/results.bin",
            "node_modules/x/y.js",
        ] {
            let f = fixture();
            fs::write(f.root.join("paper.md"), "original").unwrap();
            let session = session(&f, "edit");
            let (cp, root) = working_copy(&f, &session);
            let file = root.join(unseen);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(&file, "work the review never showed").unwrap();
            fs::write(root.join("paper.md"), "revised").unwrap();
            assistant_files::capture_session(&f.store, &session).unwrap();
            let captured = record(&f.store, &f.ws, &cp, "checkpoint").unwrap();
            let body: Checkpoint = decode(&captured).unwrap();
            assert!(!body.proposed.contains_key(unseen), "the review skips it");
            tasks::apply(
                &f.store,
                &f.ws,
                &cp,
                vec!["paper.md".into()],
                captured.revision,
            )
            .unwrap();
            let (next, _) = working_copy(&f, &session);
            assert_ne!(next, cp);
            assert_eq!(
                fs::read_to_string(&file).unwrap(),
                "work the review never showed",
                "{unseen}"
            );
        }
    }
    #[test]
    fn a_discarded_copy_goes_but_undo_can_return_before_the_next_turn() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        let (cp, root) = working_copy(&f, &session);
        fs::write(root.join("paper.md"), "revised").unwrap();
        assistant_files::capture_session(&f.store, &session).unwrap();
        let captured = record(&f.store, &f.ws, &cp, "checkpoint").unwrap();
        let app = tasks::apply(
            &f.store,
            &f.ws,
            &cp,
            vec!["paper.md".into()],
            captured.revision,
        )
        .unwrap();
        tasks::undo(&f.store, &f.ws, &app.id).unwrap();
        // Undo reopened the edit, so its copy is still the conversation's.
        assert_eq!(working_copy(&f, &session), (cp.clone(), root.clone()));
        tasks::reject(&f.store, &f.ws, &cp).unwrap();
        working_copy(&f, &session);
        assert!(!root.exists());
    }
    #[test]
    fn unaccepted_edits_keep_their_copy() {
        let f = fixture();
        fs::write(f.root.join("a.md"), "a").unwrap();
        fs::write(f.root.join("b.md"), "b").unwrap();
        // Part of the edit was never accepted.
        let partial = session(&f, "edit");
        let (cp, root) = working_copy(&f, &partial);
        fs::write(root.join("a.md"), "a2").unwrap();
        fs::write(root.join("b.md"), "b2").unwrap();
        assistant_files::capture_session(&f.store, &partial).unwrap();
        let captured = record(&f.store, &f.ws, &cp, "checkpoint").unwrap();
        tasks::apply(&f.store, &f.ws, &cp, vec!["a.md".into()], captured.revision).unwrap();
        working_copy(&f, &partial);
        assert_eq!(fs::read_to_string(root.join("b.md")).unwrap(), "b2");
        // Nobody has reviewed what changed after the last capture.
        let unreviewed = session(&f, "edit");
        let (cp, root) = working_copy(&f, &unreviewed);
        assistant_files::capture_session(&f.store, &unreviewed).unwrap();
        fs::write(root.join("b.md"), "written after review").unwrap();
        tasks::reject(&f.store, &f.ws, &cp).unwrap();
        working_copy(&f, &unreviewed);
        assert_eq!(
            fs::read_to_string(root.join("b.md")).unwrap(),
            "written after review"
        );
        let never_captured = session(&f, "edit");
        let (cp, root) = working_copy(&f, &never_captured);
        fs::write(root.join("new.md"), "never reviewed").unwrap();
        tasks::reject(&f.store, &f.ws, &cp).unwrap();
        working_copy(&f, &never_captured);
        assert!(root.join("new.md").exists());
    }
    #[test]
    fn a_copy_another_conversation_is_bound_to_stays() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let first = session(&f, "edit");
        let (cp, root) = working_copy(&f, &first);
        let other = session(&f, "edit");
        let current = f.store.session_snapshot(&other).unwrap().session;
        f.store
            .update_session(crate::workbench::store::UpdateSessionRequest {
                session_id: other,
                expected_revision: current.revision,
                operation_id: id("bind").unwrap(),
                overrides: Some(json!({"mode":"edit","projectCheckpointId":cp})),
                title: None,
                draft: None,
                archived: None,
                preset_id: None,
                paper_id: None,
                clear_paper: None,
            })
            .unwrap();
        tasks::reject(&f.store, &f.ws, &cp).unwrap();
        working_copy(&f, &first);
        assert!(root.exists());
    }
    #[test]
    fn oversized_input_fails_without_binding_a_partial_copy() {
        let f = fixture();
        let file = fs::File::create(f.root.join("data.bin")).unwrap();
        file.set_len(MAX_FILE + 1).unwrap();
        let session = session(&f, "edit");
        assert!(prepare(&f.store, &session).is_err());
        assert!(f
            .store
            .session_snapshot(&session)
            .unwrap()
            .session
            .overrides["projectCheckpointId"]
            .is_null());
        assert!(records(&f.store, &f.ws, "checkpoint").unwrap().is_empty());
    }

    #[test]
    fn concurrent_project_edits_are_never_overwritten() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        prepare(&f.store, &session).unwrap();
        let snapshot = f.store.session_snapshot(&session).unwrap();
        let cp = snapshot.session.overrides["projectCheckpointId"]
            .as_str()
            .unwrap();
        fs::write(
            f.store.runtime_root(&session).unwrap().join("paper.md"),
            "assistant",
        )
        .unwrap();
        assistant_files::capture_session(&f.store, &session).unwrap();
        fs::write(f.root.join("paper.md"), "researcher").unwrap();
        let record = record(&f.store, &f.ws, cp, "checkpoint").unwrap();
        assert!(tasks::apply(
            &f.store,
            &f.ws,
            cp,
            vec!["paper.md".into()],
            record.revision
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(f.root.join("paper.md")).unwrap(),
            "researcher"
        );
    }

    #[test]
    fn direct_edits_and_manually_scoped_copies_are_preserved() {
        let f = fixture();
        fs::write(f.root.join("paper.md"), "original").unwrap();
        let session = session(&f, "edit");
        let set = |overrides| {
            let current = f.store.session_snapshot(&session).unwrap().session;
            f.store
                .update_session(crate::workbench::store::UpdateSessionRequest {
                    session_id: session.clone(),
                    expected_revision: current.revision,
                    operation_id: id("settings").unwrap(),
                    overrides: Some(overrides),
                    title: None,
                    draft: None,
                    archived: None,
                    preset_id: None,
                    paper_id: None,
                    clear_paper: None,
                })
                .unwrap();
        };
        set(json!({"mode":"edit","fileMode":"direct"}));
        prepare(&f.store, &session).unwrap();
        assert_eq!(
            f.store.runtime_root(&session).unwrap(),
            f.root.canonicalize().unwrap()
        );
        let cp = copy(&f, &["paper.md"]);
        set(json!({"mode":"edit","projectCheckpointId":cp.id}));
        prepare(&f.store, &session).unwrap();
        assert_eq!(
            f.store
                .session_snapshot(&session)
                .unwrap()
                .session
                .overrides["projectCheckpointId"],
            cp.id
        );
        assert_eq!(records(&f.store, &f.ws, "checkpoint").unwrap().len(), 1);
    }
}
