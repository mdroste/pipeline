use super::*;

fn config() -> BuildConfig {
    serde_json::from_value(json!({"name":"Paper", "checkpointId":null, "directory":".",
        "rootDocument":"main.tex", "expectedPdf":"main.pdf", "engine":"latexmk",
        "timeoutSeconds":60, "inputs":["main.tex"], "advancedArgv":null,
        "automaticDependencies":true}))
    .unwrap()
}

#[test]
fn scans_new_sources_and_recorded_inputs_without_including_outputs() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("chapters")).unwrap();
    for path in [
        "main.tex",
        "chapters/intro.tex",
        "refs.bib",
        "figure.pdf",
        "main.pdf",
        "main.aux",
        "custom.input",
    ] {
        fs::write(root.join(path), "input").unwrap();
    }
    fs::write(root.join("main.fls"), format!("INPUT ./custom.input\nINPUT {}\nINPUT main.aux\nOUTPUT main.aux\nINPUT /outside/secret\nINPUT ../secret\n", root.join("refs.bib").display())).unwrap();
    let expected = vec![
        "chapters/intro.tex",
        "custom.input",
        "figure.pdf",
        "main.tex",
        "refs.bib",
    ];
    assert_eq!(
        discover(root, &config(), &["main.pdf".into()]).unwrap(),
        expected
    );
    fs::write(root.join("chapters/new.tex"), "new").unwrap();
    let inputs = discover(root, &config(), &["main.pdf".into()]).unwrap();
    assert!(inputs.contains(&"chapters/new.tex".into()));
    fs::remove_file(root.join("chapters/intro.tex")).unwrap();
    assert!(!discover(root, &config(), &["main.pdf".into()])
        .unwrap()
        .contains(&"chapters/intro.tex".into()));
}

#[cfg(unix)]
#[test]
fn discovery_never_follows_links_or_reads_special_files() {
    let dir = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("main.tex"), "main").unwrap();
    fs::write(external.path().join("secret.tex"), "secret").unwrap();
    std::os::unix::fs::symlink(external.path(), dir.path().join("linked")).unwrap();
    std::os::unix::fs::symlink(
        external.path().join("secret.tex"),
        dir.path().join("secret.tex"),
    )
    .unwrap();
    assert_eq!(
        discover(dir.path(), &config(), &[]).unwrap(),
        vec!["main.tex"]
    );
}

#[test]
fn old_settings_keep_their_manual_inputs() {
    let mut old = serde_json::to_value(config()).unwrap();
    old.as_object_mut().unwrap().remove("automaticDependencies");
    old.as_object_mut().unwrap().remove("executable");
    let old: BuildConfig = serde_json::from_value(old).unwrap();
    assert!(!old.automatic_dependencies);
    assert!(old.executable.is_none());
}

#[test]
fn automatic_inputs_respect_excluded_folders_and_fail_at_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("private")).unwrap();
    fs::write(dir.path().join("main.tex"), "main").unwrap();
    fs::write(dir.path().join("private/notes.tex"), "private").unwrap();
    assert_eq!(
        discover_inputs(dir.path(), &config(), &[], &["private".into()]).unwrap(),
        vec!["main.tex"]
    );
    let mut too_many = config();
    too_many.inputs = (0..1_001).map(|n| format!("{n}.tex")).collect();
    assert!(discover(dir.path(), &too_many, &[]).is_err());
}
