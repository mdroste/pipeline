use super::*;

#[test]
fn packaged_process_initializes_tools_without_system_poppler() {
    const CHILD: &str = "PIPELINE_TEST_PACKAGED_RESOURCES";
    if let Some(expected) = std::env::var_os(CHILD) {
        initialize_resources(None);
        let expected = PathBuf::from(expected).canonicalize().unwrap();
        assert_eq!(bundled_poppler_dir(), Some(expected.as_path()));
        assert_eq!(std::env::split_paths(&full_path()).next(), Some(expected));
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let (binary, resources) = if cfg!(target_os = "macos") {
        (
            "Pipeline.app/Contents/MacOS/probe",
            "Pipeline.app/Contents/Resources/resources/poppler",
        )
    } else if cfg!(windows) {
        ("install/probe.exe", "install/resources/poppler")
    } else {
        (
            "AppDir/usr/bin/probe",
            "AppDir/usr/lib/Pipeline/resources/poppler",
        )
    };
    let binary = temp.path().join(binary);
    let resources = temp.path().join(resources);
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&resources).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), &binary).unwrap();
    std::fs::write(
        resources.join(if cfg!(windows) {
            "pdftoppm.exe"
        } else {
            "pdftoppm"
        }),
        b"fixture",
    )
    .unwrap();
    #[cfg(unix)]
    let invocation = {
        let link = temp.path().join("pipeline-cli-link");
        std::os::unix::fs::symlink(&binary, &link).unwrap();
        link
    };
    #[cfg(not(unix))]
    let invocation = binary;
    let output = std::process::Command::new(invocation)
        .args([
            "--exact",
            "env::resource_tests::packaged_process_initializes_tools_without_system_poppler",
            "--nocapture",
        ])
        .current_dir(temp.path())
        .env(CHILD, resources)
        .env("PATH", "")
        .env("SHELL", "/missing-shell")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn cli_resource_layouts_do_not_depend_on_working_directory() {
    let temp = tempfile::tempdir().unwrap();
    for (platform, executable, resource) in [
        (
            "macos",
            "Pipeline.app/Contents/MacOS/pipeline-cli",
            "Pipeline.app/Contents/Resources/resources/poppler",
        ),
        (
            "windows",
            "install/pipeline-cli.exe",
            "install/resources/poppler",
        ),
        (
            "linux",
            "AppDir/usr/bin/pipeline-cli",
            "AppDir/usr/lib/Pipeline/resources/poppler",
        ),
    ] {
        let executable = temp.path().join(executable);
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        let directory = temp.path().join(resource);
        std::fs::create_dir_all(&directory).unwrap();
        let windows = platform == "windows";
        for name in if windows {
            ["pdftoppm.exe", "pdftotext.exe"]
        } else {
            ["pdftoppm", "pdftotext"]
        } {
            std::fs::write(directory.join(name), b"fixture").unwrap();
        }
        assert_eq!(
            find_poppler(resource_bases(Some(&executable), platform), windows),
            Some(directory.canonicalize().unwrap())
        );
    }
}

#[test]
fn explicit_gui_resource_directory_and_absent_bundle() {
    let temp = tempfile::tempdir().unwrap();
    assert!(find_poppler([temp.path().to_owned()], false).is_none());
    let directory = temp.path().join("poppler");
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(directory.join("pdftoppm"), b"fixture").unwrap();
    assert_eq!(
        find_poppler([temp.path().to_owned()], false),
        Some(directory.canonicalize().unwrap())
    );
}
