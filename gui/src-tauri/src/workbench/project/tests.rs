use super::super::store::{CreateSessionRequest, CreateWorkspaceRequest};
use super::test_support::*;
use super::*;
fn import(f: &Fixture, text: &str) -> research::PaperRevision {
    fs::write(f.root.join("main.md"), text).unwrap();
    research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Paper".into(),
            role: "manuscript".into(),
            path: f.root.join("main.md").to_string_lossy().into_owned(),
            operation_id: id("import").unwrap(),
        },
    )
    .unwrap()
    .revision
    .unwrap()
}
#[test]
fn mutations_have_scoped_identity_and_replay_protection() {
    let f = fixture();
    let request = ProjectMutation {
        workspace_id: f.ws.clone(),
        operation_id: "request".into(),
        action: ProjectAction::CreateTask {
            objective: "Original objective".into(),
            anchor_id: None,
            expected_outputs: vec![],
            expected_checks: vec![],
        },
    };
    let wire = serde_json::to_value(&request).unwrap();
    let roundtrip: ProjectMutation = serde_json::from_value(wire).unwrap();
    let first = mutate(&f.store, roundtrip).unwrap();
    assert_eq!(mutate(&f.store, request.clone()).unwrap(), first);
    assert_eq!(records(&f.store, &f.ws, "task").unwrap().len(), 1);
    let mut changed = request;
    changed.action = ProjectAction::CreateTask {
        objective: "Changed objective".into(),
        anchor_id: None,
        expected_outputs: vec![],
        expected_checks: vec![],
    };
    assert!(mutate(&f.store, changed).is_err());
    let other = f
        .store
        .create_workspace(CreateWorkspaceRequest {
            name: "Other".into(),
            root: None,
            operation_id: "other".into(),
        })
        .unwrap()
        .record;
    assert!(record(&f.store, &other.id, first["id"].as_str().unwrap(), "task").is_err());
}
#[test]
fn brief_preserves_accepted_state_and_excludes_rejected_or_excluded_notes() {
    let f = fixture();
    let mut chosen = String::new();
    let mut excluded = String::new();
    for (state, body) in [
        ("accepted", "Maintained assumption"),
        ("rejected", "Abandoned approach"),
        ("proposed", "Unaccepted suggestion"),
        ("accepted", "Explicitly excluded"),
    ] {
        let n = research::create_note(
            &f.store,
            research::CreateNoteRequest {
                workspace_id: f.ws.clone(),
                paper_id: None,
                kind: "assumption".into(),
                body: body.into(),
                state: Some(state.into()),
                origin: "user".into(),
                pinned: true,
                operation_id: id("note").unwrap(),
            },
            false,
        )
        .unwrap();
        if body == "Maintained assumption" {
            chosen = n.id;
        } else if body == "Explicitly excluded" {
            excluded = n.id;
        }
    }
    let settings = ProjectHomeSettings {
        brief_note_ids: vec![chosen],
        excluded_note_ids: vec![excluded],
        ..Default::default()
    };
    mutation(
        &f,
        ProjectAction::SaveHome {
            expected_revision: 0,
            settings,
        },
    );
    let context = project_context(&f.store, &f.ws).unwrap();
    assert!(context.contains("Maintained assumption"));
    assert!(!context.contains("Abandoned approach"));
    assert!(!context.contains("Unaccepted suggestion"));
    assert!(!context.contains("Explicitly excluded"));
    let reopened = Store::open_at(f.store.root_path()).unwrap();
    assert_eq!(project_context(&reopened, &f.ws).unwrap(), context);
}
#[test]
fn document_anchor_is_immutable_unicode_safe_and_mapping_is_conservative() {
    let f = fixture();
    let old = import(&f, "α before\nA repeated equation.\nAfter.");
    let text = read_document(&f.store, &f.ws, &old.id, 0, None).unwrap();
    let start = text.text.find("A repeated").unwrap();
    let quote = "A repeated equation.";
    let selection = DocumentSelection {
        revision_id: old.id.clone(),
        revision_hash: old.content_hash.clone(),
        start: Some(start),
        end: Some(start + quote.len()),
        quote: quote.into(),
        page: None,
        region: None,
    };
    let anchor = mutation(
        &f,
        ProjectAction::Annotate {
            selection: selection.clone(),
            body: "Check this derivation".into(),
        },
    );
    fs::write(f.root.join("main.md"), "Overwritten working file").unwrap();
    assert_eq!(
        read_document(&f.store, &f.ws, &old.id, 0, None)
            .unwrap()
            .text,
        text.text
    );
    let other = import(&f, "A repeated equation.\nA repeated equation.");
    assert_eq!(
        map_anchor(&f.store, &f.ws, &anchor.id, &other.id)
            .unwrap()
            .status,
        "ambiguous"
    );
    let unique = import(&f, "Now A repeated equation. elsewhere");
    assert_eq!(
        map_anchor(&f.store, &f.ws, &anchor.id, &unique.id)
            .unwrap()
            .status,
        "candidate"
    );
    assert_eq!(
        map_anchor(&f.store, &f.ws, &anchor.id, &old.id)
            .unwrap()
            .status,
        "exact"
    );
    let mut forged = selection;
    forged.quote = "fabricated evidence".into();
    assert!(annotate(&f.store, &f.ws, forged, "note".into()).is_err());
}
#[test]
fn inventory_bounds_scope_and_detects_external_changes() {
    let f = fixture();
    fs::write(f.root.join("a.md"), "A").unwrap();
    fs::create_dir(f.root.join("target")).unwrap();
    fs::write(f.root.join("target/ignored.md"), "cache").unwrap();
    let first = mutation(&f, ProjectAction::Refresh);
    let inventory: Inventory = decode(&first).unwrap();
    assert_eq!(inventory.files.len(), 1);
    fs::write(f.root.join("a.md"), "B").unwrap();
    fs::write(f.root.join("b.md"), "new").unwrap();
    let next = mutation(&f, ProjectAction::Refresh);
    let inventory: Inventory = decode(&next).unwrap();
    assert_eq!(inventory.files[0].status, "changed");
    assert_eq!(inventory.files[1].status, "added");
    fs::remove_file(f.root.join("a.md")).unwrap();
    let next = mutation(&f, ProjectAction::Refresh);
    assert!(decode::<Inventory>(&next)
        .unwrap()
        .files
        .iter()
        .any(|e| e.path == "a.md" && e.status == "missing"));
}
#[test]
#[cfg(unix)]
fn background_checks_trust_size_and_time_and_refresh_reads_everything() {
    use std::time::{Duration, UNIX_EPOCH};
    let f = fixture();
    let path = f.root.join("a.md");
    let moment = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let write = |text: &str| {
        fs::write(&path, text).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(moment)
            .unwrap();
    };
    let status = |action| {
        let inventory: Inventory = decode(&mutation(&f, action)).unwrap();
        inventory.files[0].status.clone()
    };
    write("first");
    mutation(&f, ProjectAction::Refresh);
    // The same size and time: a background check takes this for unchanged.
    write("other");
    assert_eq!(status(ProjectAction::CheckForChanges), "current");
    assert_eq!(status(ProjectAction::Refresh), "changed");
    // A new size is noticed without reading anything else.
    write("much longer");
    assert_eq!(status(ProjectAction::CheckForChanges), "changed");
}
#[test]
fn checks_for_changes_keep_no_operation_rows_and_task_inventories_keep_no_times() {
    let f = fixture();
    fs::write(f.root.join("a.md"), "A").unwrap();
    mutation(&f, ProjectAction::CheckForChanges);
    let rows: i64 = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM project_operations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    assert!(inventory_at(&f.root, &[]).unwrap().modified_ms.is_empty());
}
#[test]
fn refreshes_leave_no_operation_rows() {
    let f = fixture();
    fs::write(f.root.join("a.md"), "A").unwrap();
    let before: i64 = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM project_operations", [], |r| r.get(0))
        .unwrap();
    mutation(&f, ProjectAction::Refresh);
    mutation(&f, ProjectAction::Refresh);
    let after: i64 = f
        .store
        .connection()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM project_operations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before);
}
#[test]
#[cfg(unix)]
fn descriptor_relative_io_rejects_symlinks_and_copies_are_not_hardlinks() {
    use std::os::unix::fs::{symlink, MetadataExt};
    let f = fixture();
    let outside = f._temp.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("secret"), "private").unwrap();
    symlink(&outside, f.root.join("escape")).unwrap();
    let safe = files::SafeRoot::open(&f.root).unwrap();
    assert!(safe.read("escape/secret").is_err());
    assert!(safe
        .replace("escape/secret", None, false, Some(b"bad"), false, "test")
        .is_err());
    assert_eq!(
        fs::read_to_string(outside.join("secret")).unwrap(),
        "private"
    );
    fs::write(f.root.join("paper.md"), "paper").unwrap();
    let c = copy(&f, &["paper.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    assert_ne!(
        fs::metadata(f.root.join("paper.md")).unwrap().ino(),
        fs::metadata(Path::new(&body.task_root).join("paper.md"))
            .unwrap()
            .ino()
    );
}
#[test]
#[cfg(unix)]
fn task_session_uses_copy_root_and_rejection_retires_its_write_scope() {
    let f = fixture();
    fs::write(f.root.join("paper.md"), "paper").unwrap();
    let c = copy(&f, &["paper.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    let session = task_session(&f.store, &f.ws, &c.id).unwrap();
    assert_eq!(
        f.store.runtime_root(&session.id).unwrap(),
        PathBuf::from(body.task_root)
    );
    let effective = research::resolve_harness(&f.store, &session.id).unwrap();
    assert!(!effective
        .dynamic_tools
        .iter()
        .any(|t| t["name"] == "workbench_research_run"));
    mutation(
        &f,
        ProjectAction::Reject {
            checkpoint_id: c.id,
        },
    );
    assert!(f.store.runtime_root(&session.id).is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("paper.md")).unwrap(),
        "paper"
    );
}
#[test]
fn plain_conversation_has_no_project_context() {
    let f = fixture();
    let session = f
        .store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Plain".into(),
            operation_id: "plain".into(),
        })
        .unwrap()
        .record;
    let effective = research::resolve_harness(&f.store, &session.id).unwrap();
    assert!(effective.context_preview.is_empty());
    assert!(effective.dynamic_tools.is_empty());
}
#[test]
fn host_authorization_is_bound_to_current_inputs_and_profiles() {
    let f = fixture();
    fs::write(f.root.join("input.md"), "first").unwrap();
    let profile = research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: "Bounded fixture".into(),
            adapter: "command".into(),
            argv: vec!["/usr/bin/true".into()],
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec!["input.md".into()],
            timeout_seconds: 5,
            outputs: vec![],
            expected_revision: None,
            operation_id: "profile".into(),
        },
    )
    .unwrap();
    let preview = research::preview_host_execution(&f.store, &profile.id).unwrap();
    assert!(!preview.authorized);
    assert!(preview.boundary.contains("outside"));
    research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    assert!(
        research::preview_host_execution(&f.store, &profile.id)
            .unwrap()
            .authorized
    );
    fs::write(f.root.join("input.md"), "changed").unwrap();
    assert!(
        !research::preview_host_execution(&f.store, &profile.id)
            .unwrap()
            .authorized
    );
    assert!(
        research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).is_err()
    );
}
#[test]
fn rationale_does_not_convert_incompatible_units() {
    let left = research::ResearchResultV1 {
        result_id: "r".into(),
        estimand: "ATE".into(),
        specification_id: "s".into(),
        sample_id: "n".into(),
        estimate: 0.1,
        standard_error: None,
        confidence_interval: None,
        n: None,
        units: "fraction".into(),
        transformation: None,
        uncertainty_method: None,
        source_execution_id: "execution".into(),
        artifact_locator: "result.json".into(),
    };
    let mut right = left.clone();
    right.units = "percent".into();
    let result = research::compare_results(research::CompareResultsRequest {
        left,
        right,
        absolute_tolerance: 1.0,
        rationale: Some("I want to compare them".into()),
    })
    .unwrap();
    assert!(!result.comparable);
    assert!(!result.passed);
}
#[test]
#[cfg(unix)]
#[ignore = "Runs real Git; on this Mac run only through an authorized unsandboxed zsh test invocation"]
fn live_git_worktree_preserves_index_dirty_and_untracked_files() {
    let f = fixture();
    tasks::git(&f.root, &["init"]).unwrap();
    tasks::git(
        &f.root,
        &["config", "user.email", "fixture@example.invalid"],
    )
    .unwrap();
    tasks::git(&f.root, &["config", "user.name", "Pipeline fixture"]).unwrap();
    fs::write(f.root.join("a.md"), "committed").unwrap();
    tasks::git(&f.root, &["add", "a.md"]).unwrap();
    tasks::git(&f.root, &["commit", "-m", "fixture"]).unwrap();
    fs::write(f.root.join("a.md"), "staged").unwrap();
    tasks::git(&f.root, &["add", "a.md"]).unwrap();
    fs::write(f.root.join("a.md"), "unstaged").unwrap();
    fs::write(f.root.join("new.md"), "untracked").unwrap();
    let before_status = tasks::git(&f.root, &["status", "--porcelain"]).unwrap();
    let index = fs::read(f.root.join(".git/index")).unwrap();
    let head = fs::read(f.root.join(".git/HEAD")).unwrap();
    let task = task(&f);
    let c = mutation(
        &f,
        ProjectAction::Checkpoint {
            task_id: task.id,
            paths: vec!["a.md".into(), "new.md".into()],
            backend: "git".into(),
        },
    );
    let body: Checkpoint = decode(&c).unwrap();
    assert_eq!(
        fs::read_to_string(Path::new(&body.task_root).join("a.md")).unwrap(),
        "unstaged"
    );
    assert_eq!(fs::read(f.root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(f.root.join(".git/HEAD")).unwrap(), head);
    assert_eq!(
        tasks::git(&f.root, &["status", "--porcelain"]).unwrap(),
        before_status
    );
    mutation(
        &f,
        ProjectAction::Reject {
            checkpoint_id: c.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("new.md")).unwrap(),
        "untracked"
    );
}

#[test]
#[cfg(unix)]
fn a_git_task_copy_starts_no_hook_the_folder_names() {
    let f = fixture();
    tasks::git(&f.root, &["init"]).unwrap();
    tasks::git(
        &f.root,
        &["config", "user.email", "fixture@example.invalid"],
    )
    .unwrap();
    tasks::git(&f.root, &["config", "user.name", "Pipeline fixture"]).unwrap();
    tasks::git(&f.root, &["config", "commit.gpgsign", "false"]).unwrap();
    fs::write(f.root.join("a.md"), "committed").unwrap();
    tasks::git(&f.root, &["add", "a.md"]).unwrap();
    tasks::git(&f.root, &["commit", "-m", "fixture"]).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let marker = scratch.path().join("hook-ran");
    let command = format!("touch '{}'", marker.display());
    tasks::git(
        &f.root,
        &["config", "hook.evil.event", "reference-transaction"],
    )
    .unwrap();
    tasks::git(&f.root, &["config", "hook.evil.command", &command]).unwrap();
    // Git alone starts it for the kind of branch a task copy creates.
    std::process::Command::new("git")
        .args(["branch", "git-alone"])
        .current_dir(&f.root)
        .output()
        .unwrap();
    assert!(marker.exists(), "Git alone did not start the hook");
    fs::remove_file(&marker).unwrap();
    let task = task(&f);
    let c = mutation(
        &f,
        ProjectAction::Checkpoint {
            task_id: task.id,
            paths: vec!["a.md".into()],
            backend: "git".into(),
        },
    );
    let body: Checkpoint = decode(&c).unwrap();
    assert_eq!(
        fs::read_to_string(Path::new(&body.task_root).join("a.md")).unwrap(),
        "committed"
    );
    assert!(!marker.exists(), "the task copy started the folder's hook");
}

fn live_profile(
    f: &Fixture,
    adapter: &str,
    argv: Vec<String>,
    inputs: Vec<String>,
    outputs: Vec<String>,
    timeout: u64,
) -> research::ExecutionProfile {
    research::save_execution_profile(
        &f.store,
        research::SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: f.ws.clone(),
            name: format!("Live {adapter} fixture"),
            adapter: adapter.into(),
            argv,
            cwd: f.root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs,
            timeout_seconds: timeout,
            outputs,
            expected_revision: None,
            operation_id: id("profile").unwrap(),
        },
    )
    .unwrap()
}
fn qualify_run(f: &Fixture, profile: &research::ExecutionProfile) -> research::ResearchExecution {
    let preview = research::preview_host_execution(&f.store, &profile.id).unwrap();
    research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    research::run_execution(
        &f.store,
        research::RunExecutionRequest {
            plan_id: None,
            profile_id: profile.id.clone(),
            session_id: None,
            test_only: true,
            operation_id: id("execute").unwrap(),
        },
    )
    .unwrap()
}
fn qualification_file(f: &Fixture, name: &str) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/project-surface")
        .join(name);
    fs::copy(fixture, f.root.join(name)).unwrap();
}
fn qualification_receipt(name: &str, result: &research::ResearchExecution) {
    let folder = Path::new("/private/tmp/pipeline-pi-qualification");
    fs::create_dir_all(folder).unwrap();
    fs::write(
        folder.join(format!("{name}.json")),
        serde_json::to_vec_pretty(result).unwrap(),
    )
    .unwrap();
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "Runs configured real LaTeX and Python host tools in a disposable non-Git project"]
fn live_latex_and_quantitative_theory() {
    let f = fixture();
    for name in ["main.tex", "model.tex", "table.tex", "theory.py"] {
        qualification_file(&f, name);
    }
    let latex = live_profile(
        &f,
        "latex",
        vec![
            "/Library/TeX/texbin/pdflatex".into(),
            "-interaction=nonstopmode".into(),
            "-halt-on-error".into(),
            "main.tex".into(),
        ],
        vec!["main.tex".into(), "model.tex".into(), "table.tex".into()],
        vec!["main.pdf".into(), "main.log".into()],
        30,
    );
    let result = qualify_run(&f, &latex);
    qualification_receipt("latex", &result);
    assert_eq!(result.outcome, "completed", "{:?}", result.validation);
    let theory = live_profile(
        &f,
        "command",
        vec!["/usr/bin/python3".into(), "theory.py".into()],
        vec!["theory.py".into()],
        vec!["theory-result.json".into()],
        30,
    );
    let result = qualify_run(&f, &theory);
    qualification_receipt("theory", &result);
    assert_eq!(result.outcome, "completed");
}
#[test]
#[cfg(target_os = "macos")]
#[ignore = "Must run outside the sandbox through zsh: invokes oldstata only, including wrapper cleanup"]
fn live_oldstata_and_cooperative_cancellation() {
    let f = fixture();
    for name in ["empirical.do", "cancellation.do"] {
        qualification_file(&f, name);
    }
    let empirical = live_profile(
        &f,
        "stata",
        vec![
            "/bin/zsh".into(),
            "-lic".into(),
            format!(
                "oldstata -q -b do '{}'",
                f.root.join("empirical.do").display()
            ),
        ],
        vec!["empirical.do".into()],
        vec!["empirical.log".into(), "estimate.txt".into()],
        30,
    );
    let result = qualify_run(&f, &empirical);
    qualification_receipt("stata", &result);
    assert_eq!(
        result.outcome, "completed",
        "{:?} {:?}",
        result.validation, result.stderr
    );
    let cancelled = live_profile(
        &f,
        "stata",
        vec![
            "/bin/zsh".into(),
            "-lic".into(),
            format!(
                "oldstata -q -b do '{}'",
                f.root.join("cancellation.do").display()
            ),
        ],
        vec!["cancellation.do".into()],
        vec!["cancellation.log".into()],
        1,
    );
    let result = qualify_run(&f, &cancelled);
    qualification_receipt("stata-cancellation", &result);
    assert_eq!(result.outcome, "timed_out");
    assert_eq!(result.validation["stataWrapperReturnedAfterStop"], true);
    assert_eq!(result.validation["stataCleanupRequired"], false);
}

#[test]
#[cfg(unix)]
fn rollback_recovers_a_second_crash_and_preserves_private_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    fs::write(f.root.join("a.md"), "before").unwrap();
    fs::set_permissions(f.root.join("a.md"), fs::Permissions::from_mode(0o600)).unwrap();
    let c = copy(&f, &["a.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    fs::write(Path::new(&body.task_root).join("a.md"), "after").unwrap();
    let c = capture(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id,
            paths: vec!["a.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(
        fs::metadata(f.root.join("a.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let mut a: Application = decode(&applied).unwrap();
    a.state = "recovery_required".into();
    put(
        &f.store,
        &f.ws,
        &applied.id,
        "application",
        applied.revision,
        &a,
    )
    .unwrap();
    fs::rename(
        f.root.join("a.md"),
        f.root
            .join(format!(".pipeline-apply-recover-{}-0", applied.id)),
    )
    .unwrap();
    tasks::recover(&f.store, &f.ws, &applied.id).unwrap();
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "before");
}
#[test]
#[cfg(unix)]
fn mode_only_changes_are_reviewed_and_undone() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    fs::write(f.root.join("a.py"), "print(1)").unwrap();
    let c = copy(&f, &["a.py"]);
    let body: Checkpoint = decode(&c).unwrap();
    fs::set_permissions(
        Path::new(&body.task_root).join("a.py"),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let c = capture(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id,
            paths: vec!["a.py".into()],
            expected_revision: c.revision,
        },
    );
    assert!(files::SafeRoot::open(&f.root)
        .unwrap()
        .executable("a.py")
        .unwrap());
    tasks::undo(&f.store, &f.ws, &applied.id).unwrap();
    assert!(!files::SafeRoot::open(&f.root)
        .unwrap()
        .executable("a.py")
        .unwrap());
}
#[test]
fn unchanged_outputs_pass_only_when_nothing_a_real_change_could_rewrite_changed() {
    let f = fixture();
    fs::write(f.root.join("input.md"), "one").unwrap();
    // Like latexmk, this rewrites its output only when it decides it must.
    let profile = live_profile(
        &f,
        "command",
        vec![
            "/bin/sh".into(),
            "-c".into(),
            "[ -e frozen ] || cp input.md output.txt".into(),
        ],
        vec!["input.md".into()],
        vec!["output.txt".into()],
        5,
    );
    let built = qualify_run(&f, &profile);
    assert_eq!(built.outcome, "completed");
    assert_eq!(built.validation["upToDateOutputs"], json!([]));

    fs::write(f.root.join("frozen"), "").unwrap();
    let rebuilt = qualify_run(&f, &profile);
    assert_eq!(rebuilt.outcome, "completed", "{:?}", rebuilt.validation);
    assert_eq!(rebuilt.validation["unchangedPreexistingOutputs"], json!([]));
    assert_eq!(rebuilt.validation["upToDateOutputs"], json!(["output.txt"]));
    assert_eq!(
        rebuilt.output_manifest["artifacts"][0]["path"], "output.txt",
        "the output stays attached to the run"
    );
    assert_eq!(qualify_run(&f, &profile).outcome, "completed");

    // A real change that leaves the output alone is still caught.
    fs::write(f.root.join("input.md"), "two").unwrap();
    let stale = qualify_run(&f, &profile);
    assert_eq!(stale.outcome, "failed");
    assert_eq!(
        stale.validation["unchangedPreexistingOutputs"],
        json!(["output.txt"])
    );
    assert_eq!(stale.validation["upToDateOutputs"], json!([]));
    assert_eq!(
        fs::read_to_string(f.root.join("output.txt")).unwrap(),
        "one"
    );

    // Going back to the earlier input works: the output is what it produced.
    fs::write(f.root.join("input.md"), "one").unwrap();
    assert_eq!(qualify_run(&f, &profile).outcome, "completed");

    // Different bytes with the same modified time are not the same output.
    let times = fs::metadata(f.root.join("output.txt"))
        .unwrap()
        .modified()
        .unwrap();
    fs::write(f.root.join("output.txt"), "tampered").unwrap();
    fs::File::options()
        .write(true)
        .open(f.root.join("output.txt"))
        .unwrap()
        .set_modified(times)
        .unwrap();
    assert_eq!(qualify_run(&f, &profile).outcome, "failed");
}
#[test]
fn unproduced_outputs_cannot_pass_and_latest_runs_do_not_move_the_baseline() {
    let f = fixture();
    fs::write(f.root.join("input.md"), "input").unwrap();
    fs::write(f.root.join("old.txt"), "old output").unwrap();
    let profile = live_profile(
        &f,
        "command",
        vec!["/usr/bin/true".into()],
        vec!["input.md".into()],
        vec!["old.txt".into()],
        5,
    );
    let failed = qualify_run(&f, &profile);
    assert_eq!(failed.outcome, "failed");
    assert_eq!(
        failed.validation["unchangedPreexistingOutputs"],
        json!(["old.txt"])
    );
    let clean = live_profile(
        &f,
        "command",
        vec!["/usr/bin/true".into()],
        vec!["input.md".into()],
        vec![],
        5,
    );
    let accepted = qualify_run(&f, &clean);
    assert_eq!(accepted.outcome, "completed");
    mutation(
        &f,
        ProjectAction::SaveHome {
            expected_revision: 0,
            settings: ProjectHomeSettings {
                baseline_execution_id: Some(accepted.id.clone()),
                ..Default::default()
            },
        },
    );
    let newer = qualify_run(&f, &clean);
    assert_ne!(newer.id, accepted.id);
    assert_eq!(
        decode::<ProjectHomeSettings>(&home_record(&f.store, &f.ws).unwrap())
            .unwrap()
            .baseline_execution_id,
        Some(accepted.id)
    );
}
#[test]
fn accepted_manuscript_and_prior_note_revisions_survive_new_work() {
    let f = fixture();
    let old = import(&f, "Original accepted argument");
    mutation(
        &f,
        ProjectAction::SaveHome {
            expected_revision: 0,
            settings: ProjectHomeSettings {
                manuscript_revision_id: Some(old.id.clone()),
                ..Default::default()
            },
        },
    );
    let _new = import(&f, "New unaccepted argument");
    let session = f
        .store
        .create_session(CreateSessionRequest {
            workspace_id: Some(f.ws.clone()),
            title: "Resume".into(),
            operation_id: "resume".into(),
        })
        .unwrap()
        .record;
    let session = f
        .store
        .update_session(super::super::store::UpdateSessionRequest {
            session_id: session.id,
            expected_revision: session.revision,
            operation_id: "preset".into(),
            title: None,
            draft: None,
            overrides: None,
            archived: None,
            preset_id: Some("paper_revision".into()),
            paper_id: None,
            clear_paper: None,
        })
        .unwrap()
        .record;
    let effective = research::resolve_harness(&f.store, &session.id).unwrap();
    assert!(effective
        .context_preview
        .contains("Original accepted argument"));
    assert!(!effective
        .context_preview
        .contains("New unaccepted argument"));
    let note = research::create_note(
        &f.store,
        research::CreateNoteRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            kind: "decision".into(),
            body: "Original decision".into(),
            state: Some("accepted".into()),
            origin: "user".into(),
            pinned: true,
            operation_id: "note".into(),
        },
        false,
    )
    .unwrap();
    research::update_note(
        &f.store,
        research::UpdateNoteRequest {
            note_id: note.id,
            expected_revision: note.revision,
            body: Some("Revised decision".into()),
            state: None,
            pinned: None,
            operation_id: "edit-note".into(),
        },
    )
    .unwrap();
    assert_eq!(
        home(&f.store, &f.ws).unwrap().note_history[0]["details"]["previous"]["body"],
        "Original decision"
    );
}

#[test]
#[ignore = "Records local project inventory and reader measurements in a disposable fixture"]
fn measure_project_surface_fixture() {
    let f = fixture();
    for n in 0..1000 {
        fs::write(
            f.root.join(format!("source-{n:04}.md")),
            "A bounded research fixture.\n".repeat(40),
        )
        .unwrap();
    }
    let mut inventory_ms = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        mutation(&f, ProjectAction::Refresh);
        inventory_ms.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let revision = import(
        &f,
        &"A mathematical argument with α and β.\n".repeat(40_000),
    );
    let start = Instant::now();
    let view = read_document(&f.store, &f.ws, &revision.id, 0, None).unwrap();
    let reader_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let _ = home(&f.store, &f.ws).unwrap();
    let home_ms = start.elapsed().as_secs_f64() * 1000.0;
    let report = json!({"platform":std::env::consts::OS,"build":"Rust debug; local warm filesystem; no model","inventoryFiles":1000,"inventoryMs":inventory_ms,"readerTotalBytes":view.total_bytes,"readerReturnedBytes":view.text.len(),"readerMs":reader_ms,"homeMs":home_ms,"scope":"Backend service measurements; excludes IPC, webview, first paint, and provider latency."});
    let folder = Path::new("/private/tmp/pipeline-pi-qualification");
    fs::create_dir_all(folder).unwrap();
    fs::write(
        folder.join("performance.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
}

#[test]
fn recovery_helpers_reject_untrusted_claims_and_allow_untouched_new_directories() {
    let f = fixture();
    let safe = files::SafeRoot::open(&f.root).unwrap();
    assert!(!safe.recover_claim("not-created/new.md", "claim").unwrap());
    safe.clear_claim("not-created/new.md", "claim").unwrap();
    assert!(safe.clear_claim("existing.md", "bad/../../escape").is_err());
    assert!(files::relative("folder//file.md").is_err());
}

#[test]
fn changing_a_direct_launch_script_revokes_the_host_grant() {
    let f = fixture();
    fs::write(f.root.join("runner.py"), "print('first')").unwrap();
    fs::write(f.root.join("input.md"), "same input").unwrap();
    let profile = live_profile(
        &f,
        "command",
        vec!["/usr/bin/python3".into(), "runner.py".into()],
        vec!["input.md".into()],
        vec![],
        5,
    );
    let preview = research::preview_host_execution(&f.store, &profile.id).unwrap();
    research::authorize_host_execution(&f.store, &profile.id, &preview.fingerprint).unwrap();
    fs::write(f.root.join("runner.py"), "print('changed')").unwrap();
    assert!(
        !research::preview_host_execution(&f.store, &profile.id)
            .unwrap()
            .authorized
    );
}
#[test]
fn completed_host_launchers_do_not_leave_capture_pipes_waiting_for_descendants() {
    let f = fixture();
    fs::write(f.root.join("input.md"), "fixture").unwrap();
    let profile = live_profile(
        &f,
        "command",
        vec!["/bin/sh".into(), "-c".into(), "sleep 30 &".into()],
        vec!["input.md".into()],
        vec![],
        5,
    );
    let start = Instant::now();
    let result = qualify_run(&f, &profile);
    assert_eq!(result.outcome, "completed");
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn source_tree_capture_excludes_git_and_task_metadata() {
    let f = fixture();
    fs::write(f.root.join("paper.tex"), "Research source").unwrap();
    for directory in [".git", ".pipeline-tasks"] {
        fs::create_dir(f.root.join(directory)).unwrap();
        fs::write(
            f.root.join(directory).join("private.txt"),
            "not manuscript content",
        )
        .unwrap();
    }
    let imported = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Source tree".into(),
            role: "manuscript".into(),
            path: f.root.to_string_lossy().into_owned(),
            operation_id: "tree".into(),
        },
    )
    .unwrap();
    let manifest = imported.revision.unwrap().dependency_manifest;
    assert_eq!(manifest["files"].as_array().unwrap().len(), 1);
    assert_eq!(manifest["files"][0]["path"], "paper.tex");
}

mod capture_tests;
mod file_workspace_tests;
