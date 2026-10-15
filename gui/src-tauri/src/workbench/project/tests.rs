use super::super::store::{CreateSessionRequest, CreateWorkspaceRequest};
use super::*;
struct Fixture {
    _temp: tempfile::TempDir,
    store: Store,
    root: PathBuf,
    ws: String,
}
fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("research");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let ws = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Research".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    Fixture {
        _temp: temp,
        store,
        root,
        ws,
    }
}
fn mutation(f: &Fixture, action: ProjectAction) -> ProjectRecord {
    serde_json::from_value(
        mutate(
            &f.store,
            ProjectMutation {
                workspace_id: f.ws.clone(),
                operation_id: id("test").unwrap(),
                action,
            },
        )
        .unwrap(),
    )
    .unwrap()
}
fn task(f: &Fixture) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::CreateTask {
            objective: "Check the main result".into(),
            anchor_id: None,
            expected_outputs: vec!["Revised manuscript".into()],
            expected_checks: vec!["Inspect changes".into()],
        },
    )
}
fn copy(f: &Fixture, paths: &[&str]) -> ProjectRecord {
    let task = task(f);
    mutation(
        f,
        ProjectAction::Checkpoint {
            task_id: task.id,
            paths: paths.iter().map(|p| p.to_string()).collect(),
            backend: "copy".into(),
        },
    )
}
fn capture(f: &Fixture, c: &ProjectRecord) -> ProjectRecord {
    mutation(
        f,
        ProjectAction::CaptureChanges {
            checkpoint_id: c.id.clone(),
        },
    )
}
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
    changed.action = ProjectAction::Refresh;
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
fn task_copy_acceptance_and_undo_preserve_unrelated_work() {
    let f = fixture();
    fs::write(f.root.join("main.tex"), "dirty initial text").unwrap();
    fs::write(f.root.join("other.md"), "unrelated edit").unwrap();
    let c = copy(&f, &["main.tex"]);
    let body: Checkpoint = decode(&c).unwrap();
    let task = Path::new(&body.task_root);
    fs::write(task.join("main.tex"), "proposal").unwrap();
    fs::write(task.join("new.md"), "new artifact").unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "dirty initial text"
    );
    let c = capture(&f, &c);
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["main.tex".into(), "new.md".into()],
            expected_revision: c.revision,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "proposal"
    );
    assert_eq!(
        fs::read_to_string(f.root.join("other.md")).unwrap(),
        "unrelated edit"
    );
    mutation(
        &f,
        ProjectAction::Undo {
            application_id: applied.id,
        },
    );
    assert_eq!(
        fs::read_to_string(f.root.join("main.tex")).unwrap(),
        "dirty initial text"
    );
    assert!(!f.root.join("new.md").exists());
}
#[test]
#[cfg(unix)]
fn partial_acceptance_invalidates_checks_and_external_changes_block_overwrite() {
    let f = fixture();
    for name in ["a.md", "b.md"] {
        fs::write(f.root.join(name), "original").unwrap();
    }
    let c = copy(&f, &["a.md", "b.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    for name in ["a.md", "b.md"] {
        fs::write(Path::new(&body.task_root).join(name), "proposal").unwrap();
    }
    let c = capture(&f, &c);
    fs::write(f.root.join("b.md"), "external").unwrap();
    let result = tasks::apply(
        &f.store,
        &f.ws,
        &c.id,
        vec!["a.md".into(), "b.md".into()],
        c.revision,
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "original");
    let applied = mutation(
        &f,
        ProjectAction::Apply {
            checkpoint_id: c.id.clone(),
            paths: vec!["a.md".into()],
            expected_revision: c.revision,
        },
    );
    assert!(!decode::<Application>(&applied).unwrap().checks_valid);
    assert_eq!(fs::read_to_string(f.root.join("b.md")).unwrap(), "external");
    fs::write(f.root.join("a.md"), "later edit").unwrap();
    assert!(tasks::undo(&f.store, &f.ws, &applied.id).is_err());
    assert_eq!(
        fs::read_to_string(f.root.join("a.md")).unwrap(),
        "later edit"
    );
}
#[test]
#[cfg(unix)]
fn recovery_reconciles_crash_between_file_replacement_and_db_update() {
    let f = fixture();
    fs::write(f.root.join("a.md"), "original").unwrap();
    let c = copy(&f, &["a.md"]);
    let body: Checkpoint = decode(&c).unwrap();
    let before = body.base["a.md"].hash.clone();
    let after = blob(&f.store, &f.ws, b"proposal", "test").unwrap();
    let a = Application {
        checkpoint_id: c.id.clone(),
        root_identity: body.root_identity.clone(),
        state: "applying".into(),
        files: vec![AppliedFile {
            path: "a.md".into(),
            before: before.clone(),
            after: Some(after),
            before_executable: false,
            after_executable: false,
            claim: "crashed-0".into(),
            state: "pending".into(),
        }],
        direction: "accept".into(),
        error: None,
        checks_valid: false,
    };
    put(&f.store, &f.ws, "crashed", "application", 0, &a).unwrap();
    files::SafeRoot::open(&f.root)
        .unwrap()
        .replace(
            "a.md",
            before.as_deref(),
            false,
            Some(b"proposal"),
            false,
            "crashed-0",
        )
        .unwrap();
    let reopened = Store::open_at(f.store.root_path()).unwrap();
    let result = tasks::recover(&reopened, &f.ws, "crashed").unwrap();
    assert_eq!(decode::<Application>(&result).unwrap().state, "rolled_back");
    assert_eq!(fs::read_to_string(f.root.join("a.md")).unwrap(), "original");
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
        .join("../../docs/workbench/fixtures/project-surface")
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

#[test]
fn file_workspace_reads_academic_sources_and_rejects_escape_and_symlinks() {
    let f = fixture();
    fs::write(f.root.join("analysis.do"), "local x 1\nregress y x\n").unwrap();
    let read = |path: &str| {
        read_workspace_file(
            &f.store,
            FileReadRequest {
                workspace_id: f.ws.clone(),
                path: path.into(),
                checkpoint_id: None,
                revision_id: None,
            },
        )
    };
    let file = read("analysis.do").unwrap();
    assert!(file.editable);
    assert_eq!(file.hash, hash(file.text.as_ref().unwrap().as_bytes()));
    for path in [
        "../secret",
        "/tmp/secret",
        ".git/config",
        ".pipeline-tasks/file",
        "a\\b",
    ] {
        assert!(read(path).is_err());
    }
    std::os::unix::fs::symlink(f.root.join("analysis.do"), f.root.join("link.do")).unwrap();
    assert!(read("link.do").is_err());
    for path in [
        "code.ado",
        "code.R",
        "code.jl",
        "code.m",
        "code.py",
        "data.yaml",
    ] {
        assert!(file_workspace::editable_source(path));
    }
}

#[test]
fn file_workspace_captured_links_never_follow_live_files() {
    let f = fixture();
    let revision = import(&f, "Original captured note");
    fs::write(f.root.join("main.md"), "Changed live file").unwrap();
    fs::write(f.root.join("other.md"), "Uncaptured sibling").unwrap();
    let read = |path: &str| {
        read_workspace_file(
            &f.store,
            FileReadRequest {
                workspace_id: f.ws.clone(),
                path: path.into(),
                checkpoint_id: None,
                revision_id: Some(revision.id.clone()),
            },
        )
    };
    let file = read("main.md").unwrap();
    assert_eq!(file.text.as_deref(), Some("Original captured note"));
    assert!(!file.editable);
    assert!(file.external_path.is_none());
    assert!(read("other.md").is_err());
    let foreign = fixture();
    assert!(read_workspace_file(
        &f.store,
        FileReadRequest {
            workspace_id: foreign.ws,
            path: "main.md".into(),
            checkpoint_id: None,
            revision_id: Some(revision.id)
        }
    )
    .is_err());
}

#[test]
fn file_workspace_source_tree_assets_use_retained_hashes() {
    let f = fixture();
    fs::create_dir(f.root.join("figures")).unwrap();
    fs::write(f.root.join("main.md"), "![Figure](figures/plot.svg)").unwrap();
    fs::write(
        f.root.join("figures/plot.svg"),
        "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    let revision = research::import_paper(
        &f.store,
        research::ImportPaperRequest {
            workspace_id: f.ws.clone(),
            paper_id: None,
            title: "Tree".into(),
            role: "manuscript".into(),
            path: f.root.to_string_lossy().into_owned(),
            operation_id: id("import").unwrap(),
        },
    )
    .unwrap()
    .revision
    .unwrap();
    fs::write(f.root.join("figures/plot.svg"), "Changed").unwrap();
    let request = || FileReadRequest {
        workspace_id: f.ws.clone(),
        path: "figures/plot.svg".into(),
        checkpoint_id: None,
        revision_id: Some(revision.id.clone()),
    };
    let file = read_workspace_file(&f.store, request()).unwrap();
    assert_eq!(file.mime, "image/svg+xml");
    assert!(file.base64.is_some());
    let stored = f
        .store
        .root_path()
        .join("blobs")
        .join(format!("source-tree-{}", revision.content_hash))
        .join("figures/plot.svg");
    fs::write(stored, "Tampered").unwrap();
    assert!(read_workspace_file(&f.store, request()).is_err());
}

#[test]
fn conversation_file_links_stay_inside_the_session_runtime_root() {
    let f = fixture();
    let session = f
        .store
        .create_session(CreateSessionRequest {
            workspace_id: None,
            title: "Generated files".into(),
            operation_id: id("session").unwrap(),
        })
        .unwrap()
        .record;
    let root = f.store.runtime_root(&session.id).unwrap();
    fs::write(root.join("output.pdf"), b"%PDF fixture").unwrap();
    fs::write(f.root.join("outside.pdf"), b"%PDF outside").unwrap();

    let relative = read_conversation_file(&f.store, &session.id, "output.pdf").unwrap();
    assert_eq!(relative.path, "output.pdf");
    assert_eq!(relative.mime, "application/pdf");
    assert_eq!(relative.base64.as_deref(), Some("JVBERiBmaXh0dXJl"));
    assert_eq!(
        read_conversation_file(
            &f.store,
            &session.id,
            root.join("output.pdf").to_str().unwrap(),
        )
        .unwrap()
        .hash,
        relative.hash
    );
    let snapshot = snapshot_conversation_file(&f.store, &session.id, "output.pdf").unwrap();
    assert!(snapshot.starts_with(f.store.root_path().join("opened-conversation-files")));
    assert_eq!(fs::read(snapshot).unwrap(), b"%PDF fixture");
    assert!(read_conversation_file(&f.store, &session.id, "../outside.pdf").is_err());
    assert!(read_conversation_file(
        &f.store,
        &session.id,
        f.root.join("outside.pdf").to_str().unwrap(),
    )
    .is_err());

    std::os::unix::fs::symlink(f.root.join("outside.pdf"), root.join("linked.pdf")).unwrap();
    assert!(read_conversation_file(&f.store, &session.id, "linked.pdf").is_err());
}
