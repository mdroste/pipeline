//! Adapter argv shapes, Stata logs and receipt environment identity.

use super::adapters::{
    launch_script, parse_adapter, shell_words, stata_mode, validate_adapter_argv,
    waits_for_stata_launcher, StataMode,
};
use super::outputs::stata_log_errors;
use super::{validate_execution_outputs, ExecutionProfile};
use serde_json::json;

fn argv(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn accepts(adapter: &str, values: &[&str], outputs: &[&str], windows: bool) {
    validate_adapter_argv(adapter, &argv(values), &argv(outputs), windows)
        .unwrap_or_else(|error| panic!("{adapter} {values:?} was rejected: {}", error.message));
}

fn rejects(adapter: &str, values: &[&str], outputs: &[&str], windows: bool) -> String {
    match validate_adapter_argv(adapter, &argv(values), &argv(outputs), windows) {
        Ok(()) => panic!("{adapter} {values:?} was accepted"),
        Err(error) => error.message,
    }
}

#[test]
fn unknown_adapters_are_rejected_with_the_full_list() {
    let message = rejects("matlab", &["matlab", "-batch", "run"], &[], false);
    assert!(message.contains("notebook, or quarto"), "{message}");
}

#[test]
fn direct_stata_uses_platform_batch_flags_and_requires_its_log() {
    accepts(
        "stata",
        &["stata-mp", "-b", "do", "main.do"],
        &["main.log"],
        false,
    );
    accepts(
        "stata",
        &[
            "/Applications/StataNow/StataMP.app/Contents/MacOS/stata-mp",
            "-q",
            "-b",
            "do",
            "code/main.do",
            "first",
        ],
        &["main.log", "results/table.csv"],
        false,
    );
    accepts(
        "stata",
        &[
            r"C:\Program Files\StataNow19\StataMP-64.exe",
            "/e",
            "do",
            r"code\main.do",
        ],
        &["MAIN.LOG"],
        true,
    );
    accepts(
        "stata",
        &["StataSE-64.EXE", "/Q", "/E", "do", "main.do"],
        &["main.log"],
        true,
    );
    // Windows batch mode is /e; Unix batch mode is -b. Each rejects the other.
    assert!(rejects(
        "stata",
        &["StataMP-64.exe", "-b", "do", "main.do"],
        &["main.log"],
        true
    )
    .contains("/e do main.do"));
    assert!(rejects(
        "stata",
        &["stata-mp", "/e", "do", "main.do"],
        &["main.log"],
        false
    )
    .contains("-b do main.do"));
    let message = rejects("stata", &["stata", "-b", "do x.do"], &[], false);
    assert!(message.contains("batch mode"), "{message}");
    for bad in [
        argv(&["stata-mp", "do", "main.do"]),
        argv(&["stata-mp", "-b", "-b", "do", "main.do"]),
        argv(&["stata-mp", "-b", "run", "main.do"]),
        argv(&["stata-mp", "-b", "do", "../main.do"]),
        argv(&["stata-mp", "-b", "do", "/tmp/main.do"]),
        argv(&["stata-mp", "-b", "do", "main.txt"]),
        argv(&["stata-mp", "-b", "do", "main.do; rm -rf x"]),
        argv(&["stata-mp", "-b", "do", "main.do", "$HOME"]),
        argv(&["python3", "-b", "do", "main.do"]),
    ] {
        assert!(
            validate_adapter_argv("stata", &bad, &argv(&["main.log"]), false).is_err(),
            "{bad:?}"
        );
    }
    assert!(rejects(
        "stata",
        &["stata-mp", "-b", "do", "main.do"],
        &["other.log"],
        false
    )
    .contains("Add main.log"));
    assert!(validate_adapter_argv(
        "stata",
        &argv(&["StataMP-64.exe", "/e", "do", r"C:\main.do"]),
        &argv(&["main.log"]),
        true
    )
    .is_err());
}

#[test]
fn stata_custom_launchers_accept_shell_aliases_and_refuse_shell_syntax() {
    for command in [
        "oldstata -q -b do export_results.do",
        "stata -q -b do main.do",
        "stata -b do 'code/My Analysis.do' first",
        "oldstata -q -b do '/Users/researcher/Project One/main.do'",
        "stata18 -b do \"code/main.do\"",
        "oldstata -q -b do '/Users/o'\\''brien/main.do'",
    ] {
        accepts("stata", &["/bin/zsh", "-lic", command], &[], false);
    }
    accepts(
        "stata",
        &["/bin/bash", "-lic", "stata -b do main.do"],
        &[],
        false,
    );
    accepts(
        "stata",
        &["/bin/zsh", "-ic", "stata -b do main.do"],
        &[],
        false,
    );
    for command in [
        "oldstata -q -b do main.do; rm main.log",
        "oldstata -q -b do main.do && echo done",
        "oldstata -q -b do main.do | tee out",
        "oldstata -q -b do main.do &",
        "oldstata -q -b do $(pwd)/main.do",
        "oldstata -q -b do `pwd`/main.do",
        "oldstata -q -b do ${HOME}/main.do",
        "oldstata -q -b do main.do > out.txt",
        "oldstata -q -b do main.do\nrm x",
        "oldstata -q -b do ~/main.do",
        "oldstata -q -b do *.do",
        "oldstata -q -b do 'unterminated.do",
        "oldstata -q -b do \"a\\\"b.do\"",
        "oldstata -q do main.do",
        "oldstata -q -b do main.txt",
        "oldstata -q -b do ../main.do",
        "/usr/local/bin/stata -b do main.do",
        "=stata -b do main.do",
        "",
    ] {
        let message = rejects("stata", &["/bin/zsh", "-lic", command], &[], false);
        assert!(message.contains("custom launcher"), "{command}: {message}");
    }
    assert!(validate_adapter_argv(
        "stata",
        &argv(&["/bin/zsh", "-c", "stata -b do main.do"]),
        &[],
        false
    )
    .is_err());
    // Windows has no custom launcher mode.
    assert!(validate_adapter_argv(
        "stata",
        &argv(&["/bin/zsh", "-lic", "stata -b do main.do"]),
        &[],
        true
    )
    .is_err());
    assert_eq!(
        shell_words("stata -b do 'My File.do' a\\ b \"c d\"").unwrap(),
        argv(&["stata", "-b", "do", "My File.do", "a b", "c d"])
    );
}

#[test]
fn stata_must_use_the_stata_adapter() {
    for values in [
        argv(&["stata-mp", "-b", "do", "main.do"]),
        argv(&[
            r"C:\Program Files\Stata18\StataMP-64.exe",
            "/e",
            "do",
            "main.do",
        ]),
        argv(&["/bin/zsh", "-lic", "oldstata -q -b do main.do"]),
        argv(&["/bin/zsh", "-lic", "stata -q -b do main.do"]),
        argv(&["/bin/sh", "-c", "stata-se -b do main.do"]),
    ] {
        let windows = values[0].contains('\\');
        let message = validate_adapter_argv("command", &values, &[], windows)
            .unwrap_err()
            .message;
        assert!(message.contains("Stata adapter"), "{values:?}");
    }
    accepts("command", &["/bin/sh", "-c", "printf stata"], &[], false);
    accepts(
        "command",
        &["git", "commit", "-m", "stata fixes"],
        &[],
        false,
    );
}

#[test]
fn stata_modes_select_stop_handling() {
    assert_eq!(
        stata_mode(&argv(&["stata-mp", "-b", "do", "a.do"])),
        StataMode::Direct
    );
    let launcher = argv(&["/bin/zsh", "-lic", "oldstata -q -b do a.do"]);
    assert_eq!(stata_mode(&launcher), StataMode::CustomLauncher);
    assert!(waits_for_stata_launcher("stata", &launcher));
    assert!(!waits_for_stata_launcher("command", &launcher));
    assert!(!waits_for_stata_launcher(
        "stata",
        &argv(&["stata-mp", "-b", "do", "a.do"])
    ));
}

#[test]
fn script_adapters_accept_documented_interpreters_and_options() {
    accepts("r", &["Rscript", "analysis.R"], &[], false);
    accepts(
        "r",
        &[
            "/usr/local/bin/Rscript",
            "--vanilla",
            "code/clean.r",
            "2020",
        ],
        &[],
        false,
    );
    accepts(
        "r",
        &[
            r"C:\Program Files\R\R-4.4.1\bin\Rscript.exe",
            r"code\clean.R",
        ],
        &[],
        true,
    );
    accepts(
        "python",
        &["python3", "-u", "analysis.py", "--seed", "4"],
        &[],
        false,
    );
    accepts("python", &[".venv/bin/python", "analysis.py"], &[], false);
    accepts(
        "python",
        &["/opt/homebrew/bin/python3.12", "-I", "analysis.py"],
        &[],
        false,
    );
    accepts(
        "python",
        &[r"C:\Python312\python.exe", "analysis.py"],
        &[],
        true,
    );
    accepts(
        "julia",
        &["julia", "--project=.", "--threads=4", "analysis.jl"],
        &[],
        false,
    );
    accepts("julia", &["julia", "--project", "analysis.jl"], &[], false);

    for (adapter, values) in [
        ("r", argv(&["Rscript", "-e", "print(1)"])),
        ("r", argv(&["Rscript", "--vanilla"])),
        ("r", argv(&["R", "analysis.R"])),
        ("r", argv(&["Rscript", "analysis.py"])),
        ("r", argv(&["Rscript", "../analysis.R"])),
        ("r", argv(&["Rscript", "/tmp/analysis.R"])),
        ("python", argv(&["python3", "-c", "print(1)"])),
        ("python", argv(&["python3", "-m", "pkg.run"])),
        ("python", argv(&["python3", "analysis.py", "a;b"])),
        ("python", argv(&["python3", "analysis.py", "|", "tee"])),
        ("python", argv(&["python3", "analysis.py", "$HOME"])),
        ("python", argv(&["pythonista", "analysis.py"])),
        ("python", argv(&["python3.x", "analysis.py"])),
        ("python", argv(&["bash", "analysis.py"])),
        ("julia", argv(&["julia", "-e", "println(1)"])),
        ("julia", argv(&["julia", "analysis.R"])),
        ("julia", argv(&["julia", "--project="])),
    ] {
        assert!(
            validate_adapter_argv(adapter, &values, &[], false).is_err(),
            "{adapter} {values:?}"
        );
    }
    assert!(
        rejects("python", &["python3", "-c", "x"], &[], false).contains("Put the code in a script")
    );
    assert!(validate_adapter_argv(
        "r",
        &argv(&["Rscript.exe", r"C:\code\analysis.R"]),
        &[],
        true
    )
    .is_err());
}

#[test]
fn notebook_adapter_requires_an_executed_copy_among_outputs() {
    accepts(
        "notebook",
        &[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "analysis.executed.ipynb",
            "analysis.ipynb",
        ],
        &["analysis.executed.ipynb"],
        false,
    );
    accepts(
        "notebook",
        &[
            "jupyter",
            "nbconvert",
            "notebooks/clean.ipynb",
            "--to=html",
            "--execute",
            "--output=clean-run",
            "--ExecutePreprocessor.timeout=600",
        ],
        &["notebooks/clean-run.html"],
        false,
    );
    accepts(
        "notebook",
        &[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "run",
            "--output-dir",
            "build",
            "nb/clean.ipynb",
        ],
        &["build/run.ipynb"],
        false,
    );
    let shape = parse_adapter(
        "notebook",
        &argv(&[
            "jupyter.exe",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "run",
            r"nb\clean.ipynb",
        ]),
        true,
    )
    .unwrap();
    assert_eq!(shape.required_output.as_deref(), Some("nb/run.ipynb"));
    accepts(
        "notebook",
        &[
            "jupyter.exe",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "run",
            r"nb\clean.ipynb",
        ],
        &[r"NB\RUN.ipynb"],
        true,
    );

    let missing_output = rejects(
        "notebook",
        &[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "run",
            "a.ipynb",
        ],
        &["other.ipynb"],
        false,
    );
    assert!(missing_output.contains("Add run.ipynb"), "{missing_output}");
    let overwrite = rejects(
        "notebook",
        &[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "a",
            "a.ipynb",
        ],
        &["a.ipynb"],
        false,
    );
    assert!(overwrite.contains("replace the source"), "{overwrite}");
    for values in [
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--inplace",
            "--to",
            "notebook",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--to",
            "notebook",
            "--output",
            "b",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--output",
            "b",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "pdf",
            "--output",
            "b",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "x/b",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "b",
            "--allow-errors",
            "a.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "b",
            "a.ipynb",
            "c.ipynb",
        ]),
        argv(&[
            "jupyter",
            "nbconvert",
            "--execute",
            "--to",
            "notebook",
            "--output",
            "b",
            "../a.ipynb",
        ]),
        argv(&["jupyter", "lab", "a.ipynb"]),
        argv(&["python3", "nbconvert", "--execute", "a.ipynb"]),
    ] {
        assert!(
            validate_adapter_argv("notebook", &values, &argv(&["b.ipynb"]), false).is_err(),
            "{values:?}"
        );
    }
}

#[test]
fn quarto_adapter_renders_project_documents_to_declared_outputs() {
    accepts(
        "quarto",
        &["quarto", "render", "report.qmd"],
        &["report.html"],
        false,
    );
    accepts(
        "quarto",
        &[
            "quarto",
            "render",
            "paper/paper.qmd",
            "--to",
            "pdf",
            "--execute",
            "-P",
            "seed:1",
        ],
        &["paper/paper.pdf"],
        false,
    );
    accepts(
        "quarto",
        &["quarto", "render", "notes.Rmd", "--output-dir=_site"],
        &["_site/notes.html"],
        false,
    );
    assert!(
        rejects("quarto", &["quarto", "render", "report.qmd"], &[], false)
            .contains("rendered document")
    );
    for values in [
        argv(&["quarto", "preview", "report.qmd"]),
        argv(&["quarto", "render"]),
        argv(&["quarto", "render", "--to", "html"]),
        argv(&["quarto", "render", "report.docx"]),
        argv(&["quarto", "render", "report.qmd", "--output", "-"]),
        argv(&["quarto", "render", "report.qmd", "--output-dir", "../out"]),
        argv(&["quarto", "render", "report.qmd", "--unknown"]),
        argv(&["quarto", "render", "report.qmd", "--to"]),
        argv(&["quarto", "render", "report.qmd;", "--to", "html"]),
    ] {
        assert!(
            validate_adapter_argv("quarto", &values, &argv(&["report.html"]), false).is_err(),
            "{values:?}"
        );
    }
}

#[test]
fn launch_scripts_are_known_for_structured_adapters_only() {
    assert_eq!(
        launch_script("r", &argv(&["Rscript", "--vanilla", "a.R"]), false).as_deref(),
        Some("a.R")
    );
    assert_eq!(
        launch_script(
            "stata",
            &argv(&["stata-mp", "-b", "do", "code/a.do"]),
            false
        )
        .as_deref(),
        Some("code/a.do")
    );
    assert_eq!(
        launch_script(
            "stata",
            &argv(&["/bin/zsh", "-lic", "stata -b do a.do"]),
            false
        ),
        None
    );
    assert_eq!(
        launch_script("command", &argv(&["python3", "a.py"]), false),
        None
    );
}

#[test]
fn stata_log_parsing_reports_codes_and_messages_once() {
    let log = "\
. use data, clear
. regress y x
variable y not found
r(111);

end of do-file
r(111);
";
    assert_eq!(
        stata_log_errors(log),
        vec![json!({"code":111,"message":"variable y not found"})]
    );
    assert!(
        stata_log_errors(". display 1\n1\n. display \"r(198);\"\nr(198); is text\n").is_empty()
    );
    let windows = "invalid syntax\r\nr(198);\r\n";
    assert_eq!(stata_log_errors(windows)[0]["code"], 198);
    let latin1 = String::from_utf8_lossy(b"Donn\xe9es manquantes\nr(2000);\n").into_owned();
    assert_eq!(stata_log_errors(&latin1)[0]["code"], 2000);
    let many = (1..40)
        .map(|code| format!("e{code}\nr({code});\n"))
        .collect::<String>();
    assert_eq!(stata_log_errors(&many).len(), 20);
}

#[test]
fn stata_output_validation_records_log_errors_for_direct_profiles() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("main.log"), "no observations\nr(2000);\n").unwrap();
    let profile = ExecutionProfile {
        id: "stata".into(),
        workspace_id: "workspace".into(),
        name: "Stata".into(),
        adapter: "stata".into(),
        argv: argv(&["stata-mp", "-b", "do", "main.do"]),
        cwd: temp.path().to_string_lossy().into_owned(),
        environment: json!({}),
        inputs: vec![],
        timeout_seconds: 30,
        outputs: argv(&["main.log"]),
        tested_at: None,
        test_status: None,
        revision: 1,
    };
    let (failed, valid) = validate_execution_outputs(&profile, "completed");
    assert!(!valid);
    assert_eq!(failed["checks"][1]["check"], "stataLogHasNoErrorCode");
    assert_eq!(failed["checks"][1]["logs"][0]["errors"][0]["code"], 2000);
    assert_eq!(
        failed["checks"][1]["logs"][0]["errors"][0]["message"],
        "no observations"
    );
    std::fs::write(temp.path().join("main.log"), b"Donn\xe9es pr\xeates\n").unwrap();
    let (passed, valid) = validate_execution_outputs(&profile, "completed");
    assert!(valid, "{passed}");
}

#[cfg(unix)]
mod receipts {
    use super::argv;
    use crate::workbench::research::execution::{
        authorize_host_execution, preview_host_execution, run_execution, save_execution_profile,
        RunExecutionRequest, SaveExecutionProfileRequest,
    };
    use crate::workbench::store::{CreateWorkspaceRequest, Store};
    use serde_json::json;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn executable(path: &Path, source: &str) {
        fs::write(path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A fake Rscript answers --version on stderr like R does, then runs the
    /// "script" by writing the declared output.
    #[test]
    fn r_receipts_record_renv_lock_and_runtime_version() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let tools = temp.path().join("tools");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&tools).unwrap();
        let rscript = tools.join("Rscript");
        executable(
            &rscript,
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'Rscript (R) version 4.4.1 (2024-06-14)' >&2; exit 0; fi\nprintf 'estimate,1.0\\n' > results.csv\n",
        );
        fs::write(root.join("analysis.R"), "write.csv(1, 'results.csv')\n").unwrap();
        fs::write(root.join("renv.lock"), "{\"R\":{\"Version\":\"4.4.1\"}}\n").unwrap();
        let store = Store::open_at(&temp.path().join("store")).unwrap();
        let workspace_id = store
            .create_workspace(CreateWorkspaceRequest {
                name: "R receipts".into(),
                root: Some(root.to_string_lossy().into_owned()),
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let request = SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: workspace_id.clone(),
            name: "R analysis".into(),
            adapter: "r".into(),
            argv: vec![rscript.to_string_lossy().into_owned(), "analysis.R".into()],
            cwd: root.to_string_lossy().into_owned(),
            environment: json!({}),
            // Undeclared on purpose: the launch identity alone must cover the
            // script and the lockfile.
            inputs: vec![],
            timeout_seconds: 10,
            outputs: argv(&["results.csv"]),
            expected_revision: None,
            operation_id: "save-r".into(),
        };
        let profile = save_execution_profile(&store, request).unwrap();
        let preview = preview_host_execution(&store, &profile.id).unwrap();
        let identity = &preview.launch["environmentIdentity"];
        assert_eq!(identity["files"][0]["path"], "renv.lock");
        assert_eq!(
            identity["files"][0]["contentHash"].as_str().unwrap().len(),
            64
        );
        assert!(preview.launch["launchFiles"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("analysis.R"));
        authorize_host_execution(&store, &profile.id, &preview.fingerprint).unwrap();
        let receipt = run_execution(
            &store,
            RunExecutionRequest {
                plan_id: None,
                profile_id: profile.id.clone(),
                session_id: None,
                test_only: true,
                operation_id: "test-r".into(),
            },
        )
        .unwrap();
        assert_eq!(receipt.outcome, "completed", "{}", receipt.validation);
        let runtime = &receipt.validation["runtimeIdentity"];
        assert_eq!(runtime["kind"], "r");
        assert_eq!(runtime["version"], "4.4.1");
        assert_eq!(
            runtime["versionText"],
            "Rscript (R) version 4.4.1 (2024-06-14)"
        );
        assert_eq!(
            receipt.validation["authorizedLaunch"]["environmentIdentity"]["files"][0]["path"],
            "renv.lock"
        );
        assert_eq!(
            receipt.validation["stataLaunchMode"],
            serde_json::Value::Null
        );

        // A changed lockfile is a changed environment and needs a new review.
        fs::write(root.join("renv.lock"), "{\"R\":{\"Version\":\"4.5.0\"}}\n").unwrap();
        assert_ne!(
            preview_host_execution(&store, &profile.id)
                .unwrap()
                .fingerprint,
            preview.fingerprint
        );

        // A missing script is reported before review.
        fs::remove_file(root.join("analysis.R")).unwrap();
        let missing = preview_host_execution(&store, &profile.id).unwrap_err();
        assert!(
            missing.message.contains("analysis.R was not found"),
            "{}",
            missing.message
        );
    }

    #[test]
    fn existing_command_and_launcher_profiles_keep_their_launch_identity() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("requirements.txt"), "numpy==2.0\n").unwrap();
        let store = Store::open_at(&temp.path().join("store")).unwrap();
        let workspace_id = store
            .create_workspace(CreateWorkspaceRequest {
                name: "Compatibility".into(),
                root: Some(root.to_string_lossy().into_owned()),
                operation_id: "create".into(),
            })
            .unwrap()
            .record
            .id;
        let profile = save_execution_profile(
            &store,
            SaveExecutionProfileRequest {
                profile_id: None,
                workspace_id,
                name: "Command".into(),
                adapter: "command".into(),
                argv: argv(&["/bin/sh", "-c", "true"]),
                cwd: root.to_string_lossy().into_owned(),
                environment: json!({}),
                inputs: vec![],
                timeout_seconds: 5,
                outputs: vec![],
                expected_revision: None,
                operation_id: "save".into(),
            },
        )
        .unwrap();
        let launch = preview_host_execution(&store, &profile.id).unwrap().launch;
        assert!(launch.get("environmentIdentity").is_none(), "{launch}");
    }
}

/// Stores created before migration 16 reject structured adapters in SQL; the
/// rebuilt table keeps every profile and its references.
#[test]
fn migration_16_keeps_profiles_and_accepts_structured_adapters() {
    use crate::workbench::research::execution::{
        list_execution_profiles, save_execution_profile, SaveExecutionProfileRequest,
    };
    use crate::workbench::store::{CreateWorkspaceRequest, Store};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let store_root = temp.path().join("store");
    let store = Store::open_at(&store_root).unwrap();
    let workspace_id = store
        .create_workspace(CreateWorkspaceRequest {
            name: "Migration".into(),
            root: Some(root.to_string_lossy().into_owned()),
            operation_id: "create".into(),
        })
        .unwrap()
        .record
        .id;
    let request = |adapter: &str, values: &[&str], outputs: &[&str], operation: &str| {
        SaveExecutionProfileRequest {
            profile_id: None,
            workspace_id: workspace_id.clone(),
            name: format!("{adapter} profile"),
            adapter: adapter.into(),
            argv: argv(values),
            cwd: root.to_string_lossy().into_owned(),
            environment: json!({}),
            inputs: vec![],
            timeout_seconds: 30,
            outputs: argv(outputs),
            expected_revision: None,
            operation_id: operation.into(),
        }
    };
    let legacy = save_execution_profile(
        &store,
        request(
            "stata",
            if cfg!(windows) {
                &["StataMP-64.exe", "/e", "do", "main.do"]
            } else {
                &["/bin/zsh", "-lic", "oldstata -q -b do main.do"]
            },
            &["main.log"],
            "legacy",
        ),
    )
    .unwrap();
    drop(store);
    // Recreate the version 15 table definition around the existing row.
    let connection = rusqlite::Connection::open(store_root.join("research.sqlite3")).unwrap();
    connection
        .execute_batch(
            "DROP INDEX transcript_items_page;
             PRAGMA foreign_keys=OFF;
             CREATE TABLE old_profiles AS SELECT * FROM execution_profiles;
             DROP TABLE execution_profiles;
             CREATE TABLE execution_profiles (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                adapter TEXT NOT NULL CHECK (adapter IN ('latex', 'stata', 'command')),
                argv_json TEXT NOT NULL, cwd TEXT NOT NULL, environment_json TEXT NOT NULL,
                inputs_json TEXT NOT NULL,
                timeout_seconds INTEGER NOT NULL CHECK (timeout_seconds BETWEEN 1 AND 7200),
                outputs_json TEXT NOT NULL, tested_at TEXT, test_status TEXT,
                revision INTEGER NOT NULL DEFAULT 1 CHECK (revision >= 1),
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
             INSERT INTO execution_profiles SELECT * FROM old_profiles;
             DROP TABLE old_profiles;
             CREATE INDEX execution_profiles_workspace ON execution_profiles(workspace_id, name);
             DROP TABLE conversation_tool_receipts;
             PRAGMA user_version=15;",
        )
        .unwrap();
    assert!(connection
        .execute(
            "UPDATE execution_profiles SET adapter='python' WHERE id=?1",
            [&legacy.id]
        )
        .is_err());
    drop(connection);

    let store = Store::open_at(&store_root).unwrap();
    let profiles = list_execution_profiles(&store, &workspace_id).unwrap();
    assert_eq!(profiles, vec![legacy.clone()]);
    std::fs::write(root.join("analysis.py"), "print(1)\n").unwrap();
    let python = save_execution_profile(
        &store,
        request("python", &["python3", "analysis.py"], &[], "python"),
    )
    .unwrap();
    assert_eq!(python.adapter, "python");
    let connection = rusqlite::Connection::open(store_root.join("research.sqlite3")).unwrap();
    let index: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='execution_profiles_workspace'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index, 1);
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(violations, 0);
}
