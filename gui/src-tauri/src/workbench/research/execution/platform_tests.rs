use super::*;

fn profile(cwd: &Path, argv: Vec<String>, environment: Value) -> ExecutionProfile {
    ExecutionProfile {
        id: "test-profile".into(),
        workspace_id: "test-workspace".into(),
        name: "Test".into(),
        adapter: "command".into(),
        argv,
        cwd: cwd.to_string_lossy().into_owned(),
        environment,
        inputs: vec![],
        timeout_seconds: 5,
        outputs: vec![],
        tested_at: None,
        test_status: None,
        revision: 1,
    }
}

#[test]
fn windows_latex_names_share_one_classifier() {
    for engine in ["pdflatex", "xelatex", "lualatex", "tectonic", "latexmk"] {
        for name in [
            engine.to_owned(),
            format!("{engine}.exe"),
            format!("C:\\TeX α\\{}.EXE", engine.to_ascii_uppercase()),
        ] {
            assert_eq!(latex_engine(&name, true).as_deref(), Some(engine));
        }
        assert_eq!(
            latex_engine(&format!("/opt/tex/{engine}"), false).as_deref(),
            Some(engine)
        );
    }
    for name in [
        "cmd.exe",
        "pdflatex.cmd",
        "eviltectonic",
        "pdflatex.exe.bat",
    ] {
        assert_eq!(latex_engine(name, true), None);
    }
    assert_eq!(latex_engine("PDFLATEX", false), None);
}

#[test]
fn latex_profiles_inject_engine_specific_flags_and_reject_shell_escape() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let store = Store::open_at(&temp.path().join("store")).unwrap();
    let workspace = store
        .create_workspace(crate::workbench::store::CreateWorkspaceRequest {
            name: "Test".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record;
    for engine in ["pdflatex", "tectonic", "latexmk"] {
        let name = if cfg!(windows) {
            format!("{}.EXE", engine.to_ascii_uppercase())
        } else {
            engine.into()
        };
        let mut request = SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: workspace.id.clone(),
            name: engine.into(),
            adapter: "latex".into(),
            argv: vec![name, "main.tex".into()],
            cwd: root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            outputs: vec![],
            timeout_seconds: 5,
            expected_revision: None,
            operation_id: format!("save-{engine}"),
        };
        let saved = save_execution_profile(&store, request.clone()).unwrap();
        assert_eq!(
            saved.argv.iter().any(|s| s == "-no-shell-escape"),
            engine != "tectonic"
        );
        assert_eq!(saved.argv.iter().any(|s| s == "-norc"), engine == "latexmk");
        request.argv.push("--shell-escape".into());
        request.operation_id.push_str("-unsafe");
        assert!(save_execution_profile(&store, request).is_err());
    }
}

#[cfg(unix)]
fn executable(path: &Path, source: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, source).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn recovered_path_is_used_for_preview_child_helpers_and_authorization() {
    const CHILD: &str = "PIPELINE_TEST_LOGIN_PATH";
    if let Some(path) = std::env::var_os(CHILD) {
        let root = PathBuf::from(path);
        let mut p = profile(&root, vec!["audit-run".into()], json!({}));
        let launch = launch_manifest(&p).unwrap();
        assert_eq!(
            Path::new(launch["invocationPath"].as_str().unwrap()),
            root.join("audit-run")
        );
        let result = run_profile_process("path-test", &p, &launch).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            "helper found"
        );
        p.environment = json!({"PATH": "/missing"});
        assert!(launch_manifest(&p).is_err());
        p.argv[0] = root.join("audit-run").to_string_lossy().into_owned();
        let overridden = launch_manifest(&p).unwrap();
        assert_eq!(overridden["path"], "/missing");
        assert!(run_profile_process("changed-path", &p, &launch).is_err());
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    executable(&root.join("audit-run"), "#!/bin/sh\nexec audit-helper\n");
    executable(
        &root.join("audit-helper"),
        "#!/bin/sh\nprintf 'helper found\\n'\n",
    );
    executable(
        &root.join("login-shell"),
        "#!/bin/sh\nprintf '%s\\n' \"$PIPELINE_TEST_LOGIN_PATH\"\n",
    );
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "workbench::research::execution::platform_tests::recovered_path_is_used_for_preview_child_helpers_and_authorization", "--nocapture"])
        .env(CHILD, &root).env("SHELL", root.join("login-shell")).env("PATH", "/usr/bin:/bin").output().unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn windows_preview_and_execution_resolve_the_same_extensionless_program() {
    let temp = tempfile::tempdir().unwrap();
    let p = profile(temp.path(), vec!["whoami".into()], json!({}));
    let launch = launch_manifest(&p).unwrap();
    assert!(launch["invocationPath"]
        .as_str()
        .unwrap()
        .to_ascii_lowercase()
        .ends_with("whoami.exe"));
    let output = run_profile_process("windows-resolution", &p, &launch).unwrap();
    assert!(!output.stdout.is_empty());
}
