//! Detection over temporary install layouts; never the real machine's tools.

use super::locations::{Host, Platform};
use super::probe::{
    extract_version, first_line, folder_version, matlab_release, plist_version,
    stata_install_version,
};
use super::project::pyvenv_version;
use super::{detect_with, environment_identity, DetectedToolchain, ToolchainReport};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn host(platform: Platform) -> Host {
    Host {
        platform,
        path: OsString::new(),
        pathext: None,
        home: None,
        applications: Vec::new(),
        frameworks: Vec::new(),
        program_files: Vec::new(),
        user_programs: Vec::new(),
        prefixes: Vec::new(),
        probe_versions: false,
    }
}

fn file(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn nested(base: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn tools<'a>(report: &'a ToolchainReport, kind: &str) -> Vec<&'a DetectedToolchain> {
    report
        .tools
        .iter()
        .filter(|tool| tool.kind == kind)
        .collect()
}

#[test]
fn version_text_parsing_is_bounded_and_specific() {
    assert_eq!(
        extract_version("Rscript (R) version 4.4.1 (2024-06-14)").as_deref(),
        Some("4.4.1")
    );
    assert_eq!(extract_version("Python 3.12.4").as_deref(), Some("3.12.4"));
    assert_eq!(
        extract_version("julia version 1.10.4").as_deref(),
        Some("1.10.4")
    );
    assert_eq!(extract_version("1.5.57").as_deref(), Some("1.5.57"));
    assert_eq!(folder_version("Stata18").as_deref(), Some("18"));
    assert_eq!(folder_version("StataNow"), None);
    assert_eq!(folder_version("R-4.4.1").as_deref(), Some("4.4.1"));
    assert_eq!(
        matlab_release("MATLAB_R2024b.app").as_deref(),
        Some("R2024b")
    );
    assert_eq!(matlab_release("MATLAB"), None);
    assert_eq!(
        first_line(b"\n\n  Python 3.12.4\x1b[0m  \nextra").as_deref(),
        Some("Python 3.12.4[0m")
    );
    assert_eq!(first_line(&[b'7'; 1000]).unwrap().len(), 200);
    assert_eq!(first_line(b"   \n"), None);
}

#[test]
fn install_metadata_supplies_stata_and_python_environment_versions() {
    let temp = tempfile::tempdir().unwrap();
    let app = nested(temp.path(), &["Applications", "StataNow", "StataMP.app"]);
    file(
        &nested(&app, &["Contents", "Info.plist"]),
        "<plist><dict><key>CFBundleName</key><string>StataMP</string>\n<key>CFBundleShortVersionString</key>\n\t<string>19.5</string></dict></plist>",
    );
    assert_eq!(
        plist_version(&nested(&app, &["Contents", "Info.plist"])).as_deref(),
        Some("19.5")
    );
    assert_eq!(
        stata_install_version(&nested(&app, &["Contents", "MacOS", "stata-mp"])),
        Some(("19.5".into(), "install folder (Info.plist)"))
    );
    assert_eq!(
        stata_install_version(Path::new("/usr/local/stata18/stata-mp")),
        Some(("18".into(), "install folder name"))
    );
    let cfg = temp.path().join("pyvenv.cfg");
    fs::write(
        &cfg,
        "home = /usr/bin\ninclude-system-site-packages = false\nversion_info = 3.12.4\n",
    )
    .unwrap();
    assert_eq!(pyvenv_version(&cfg).as_deref(), Some("3.12.4"));
    fs::write(&cfg, "home = /usr/bin\nversion = 3.11.9\n").unwrap();
    assert_eq!(pyvenv_version(&cfg).as_deref(), Some("3.11.9"));
}

#[test]
fn windows_layouts_find_stata_r_python_quarto_and_matlab() {
    let temp = tempfile::tempdir().unwrap();
    let program_files = temp.path().join("Program Files");
    let programs = nested(temp.path(), &["Local", "Programs"]);
    file(
        &nested(&program_files, &["StataNow19", "StataMP-64.exe"]),
        "",
    );
    file(&nested(&program_files, &["StataNow19", "unins000.exe"]), "");
    file(&nested(&program_files, &["Stata18", "StataSE-64.exe"]), "");
    file(
        &nested(&program_files, &["R", "R-4.4.1", "bin", "Rscript.exe"]),
        "",
    );
    file(
        &nested(&program_files, &["Quarto", "bin", "quarto.exe"]),
        "",
    );
    file(
        &nested(&program_files, &["MATLAB", "R2023a", "bin", "matlab.exe"]),
        "",
    );
    file(
        &nested(&programs, &["Python", "Python312", "python.exe"]),
        "",
    );
    file(
        &nested(
            &programs,
            &["Python", "Python312", "Scripts", "jupyter.exe"],
        ),
        "",
    );
    file(
        &nested(&programs, &["Julia-1.10.4", "bin", "julia.exe"]),
        "",
    );
    let mut windows = host(Platform::Windows);
    windows.program_files = vec![program_files.clone()];
    windows.user_programs = vec![programs];
    let report = detect_with(&windows, None);
    assert_eq!(report.platform, "windows");
    let stata = tools(&report, "stata");
    assert_eq!(stata.len(), 2, "{stata:?}");
    assert_eq!(stata[0].label, "Stata18 SE");
    assert_eq!(stata[1].label, "StataNow19 MP");
    assert_eq!(stata[1].version.as_deref(), Some("19"));
    assert_eq!(stata[1].version_source, "install folder name");
    assert_eq!(stata[1].adapter.as_deref(), Some("stata"));
    assert!(stata[1]
        .note
        .as_deref()
        .unwrap()
        .contains("does not start Stata"));
    assert_eq!(tools(&report, "r")[0].version.as_deref(), Some("4.4.1"));
    assert_eq!(tools(&report, "python").len(), 1);
    assert_eq!(
        tools(&report, "jupyter")[0].adapter.as_deref(),
        Some("notebook")
    );
    assert_eq!(
        tools(&report, "julia")[0].version.as_deref(),
        Some("1.10.4")
    );
    assert_eq!(tools(&report, "quarto").len(), 1);
    let matlab = tools(&report, "matlab");
    assert_eq!(matlab[0].version.as_deref(), Some("R2023a"));
    assert_eq!(matlab[0].adapter, None);
    // Empty executables still hash; the sum is SHA-256 of zero bytes.
    assert_eq!(
        stata[0].sha256.as_deref(),
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
}

#[cfg(unix)]
mod unix {
    use super::*;

    #[test]
    fn mac_layouts_offer_console_stata_binaries_with_bundle_versions() {
        let temp = tempfile::tempdir().unwrap();
        let applications = temp.path().join("Applications");
        let frameworks = temp.path().join("Frameworks");
        let now = nested(&applications, &["StataNow", "StataMP.app"]);
        file(&nested(&now, &["Contents", "MacOS", "stata-mp"]), "");
        file(&nested(&now, &["Contents", "MacOS", "StataMP"]), "");
        file(
            &nested(&now, &["Contents", "Info.plist"]),
            "<key>CFBundleShortVersionString</key><string>19.5</string>",
        );
        file(
            &nested(
                &applications,
                &["Stata18", "StataSE.app", "Contents", "MacOS", "stata-se"],
            ),
            "",
        );
        file(
            &nested(
                &applications,
                &[
                    "Julia-1.10.app",
                    "Contents",
                    "Resources",
                    "julia",
                    "bin",
                    "julia",
                ],
            ),
            "",
        );
        file(&nested(&applications, &["quarto", "bin", "quarto"]), "");
        file(
            &nested(&applications, &["MATLAB_R2024b.app", "bin", "matlab"]),
            "",
        );
        file(
            &nested(&frameworks, &["R.framework", "Resources", "bin", "Rscript"]),
            "",
        );
        file(
            &nested(
                &frameworks,
                &["Python.framework", "Versions", "3.12", "bin", "python3"],
            ),
            "",
        );
        file(
            &nested(
                &frameworks,
                &["Python.framework", "Versions", "3.12", "bin", "jupyter"],
            ),
            "",
        );
        // Not executable: never offered.
        let plain = nested(
            &frameworks,
            &["Python.framework", "Versions", "3.11", "bin", "python3"],
        );
        fs::create_dir_all(plain.parent().unwrap()).unwrap();
        fs::write(&plain, "").unwrap();
        let mut mac = host(Platform::MacOs);
        mac.applications = vec![applications];
        mac.frameworks = vec![frameworks];
        let report = detect_with(&mac, None);
        let stata = tools(&report, "stata");
        assert_eq!(stata.len(), 2, "{stata:?}");
        assert_eq!(stata[0].label, "Stata18 SE");
        assert_eq!(stata[0].version.as_deref(), Some("18"));
        assert_eq!(stata[1].label, "StataNow MP");
        assert!(stata[1].path.ends_with("Contents/MacOS/stata-mp"));
        assert_eq!(stata[1].version.as_deref(), Some("19.5"));
        assert_eq!(stata[1].version_source, "install folder (Info.plist)");
        let python = tools(&report, "python");
        assert_eq!(python.len(), 1);
        assert_eq!(python[0].version.as_deref(), Some("3.12"));
        assert_eq!(tools(&report, "jupyter").len(), 1);
        assert_eq!(tools(&report, "julia")[0].version.as_deref(), Some("1.10"));
        assert_eq!(tools(&report, "matlab")[0].label, "MATLAB R2024b");
        assert_eq!(tools(&report, "r")[0].label, "R (CRAN installer)");
        assert_eq!(tools(&report, "quarto").len(), 1);
    }

    #[test]
    fn linux_layouts_and_path_entries_are_deduplicated() {
        let temp = tempfile::tempdir().unwrap();
        let prefix = temp.path().join("opt");
        let bin = temp.path().join("bin");
        file(&nested(&prefix, &["stata18", "stata-mp"]), "");
        file(&nested(&prefix, &["julia-1.10.4", "bin", "julia"]), "");
        file(&nested(&prefix, &["R", "4.4.1", "bin", "Rscript"]), "");
        file(&nested(&prefix, &["quarto", "bin", "quarto"]), "");
        fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(
            nested(&prefix, &["quarto", "bin", "quarto"]),
            bin.join("quarto"),
        )
        .unwrap();
        file(&bin.join("python3"), "");
        let mut linux = host(Platform::Linux);
        linux.prefixes = vec![prefix];
        linux.path = bin.clone().into_os_string();
        let report = detect_with(&linux, None);
        let stata = tools(&report, "stata");
        assert_eq!(stata.len(), 1);
        assert_eq!(stata[0].label, "stata18 MP");
        assert_eq!(stata[0].version.as_deref(), Some("18"));
        let quarto = tools(&report, "quarto");
        assert_eq!(
            quarto.len(),
            1,
            "a PATH symlink to a standard install is listed once"
        );
        assert_eq!(quarto[0].source, "standardLocation");
        let python = tools(&report, "python");
        assert_eq!(python[0].source, "path");
        assert_eq!(python[0].label, "python3 on PATH");
        assert_eq!(tools(&report, "r")[0].version.as_deref(), Some("4.4.1"));
        assert_eq!(
            tools(&report, "julia")[0].version.as_deref(),
            Some("1.10.4")
        );
    }

    #[test]
    fn version_probes_answer_once_and_licensed_programs_are_never_started() {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        let marker = temp.path().join("stata-started");
        file(
            &bin.join("julia"),
            "#!/bin/sh\n[ \"$1\" = --version ] && echo 'julia version 1.10.4' && exit 0\nexit 3\n",
        );
        file(&bin.join("Rscript"), "#!/bin/sh\nexit 1\n");
        file(
            &bin.join("stata-mp"),
            &format!("#!/bin/sh\ntouch '{}'\necho 'Stata 18'\n", marker.display()),
        );
        file(
            &bin.join("matlab"),
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        let mut linux = host(Platform::Linux);
        linux.path = bin.into_os_string();
        linux.probe_versions = true;
        let report = detect_with(&linux, None);
        let julia = tools(&report, "julia");
        assert_eq!(julia[0].version.as_deref(), Some("1.10.4"));
        assert_eq!(
            julia[0].version_text.as_deref(),
            Some("julia version 1.10.4")
        );
        assert_eq!(julia[0].version_source, "julia --version");
        let r = tools(&report, "r");
        assert_eq!(r[0].version, None);
        assert_eq!(r[0].version_source, "the version check did not answer");
        let stata = tools(&report, "stata");
        assert_eq!(stata[0].version, None);
        assert!(stata[0].version_source.contains("licensed"));
        assert_eq!(tools(&report, "matlab").len(), 1);
        assert!(!marker.exists(), "detection must not start Stata or MATLAB");
    }

    #[test]
    fn project_environments_list_lockfiles_and_virtual_environments_first() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        file(&root.join("renv.lock"), "{}");
        file(&root.join("uv.lock"), "version = 1");
        file(&root.join("Project.toml"), "name = \"Study\"");
        file(
            &nested(&root, &[".venv", "pyvenv.cfg"]),
            "home = /usr/bin\nversion_info = 3.12.4\n",
        );
        file(&nested(&root, &[".venv", "bin", "python"]), "");
        file(&nested(&root, &[".venv", "bin", "jupyter"]), "");
        fs::create_dir_all(root.join("ado")).unwrap();
        let bin = temp.path().join("bin");
        file(&bin.join("python3"), "");
        let mut linux = host(Platform::Linux);
        linux.path = bin.into_os_string();
        let report = detect_with(&linux, Some(&root));
        let project = report.project.as_ref().unwrap();
        let names = project
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["renv.lock", "uv.lock", "Project.toml"]);
        assert_eq!(project.files[0].ecosystem, "R");
        assert_eq!(project.files[0].sha256.as_deref().unwrap().len(), 64);
        assert!(project.stata_ado_folder);
        assert_eq!(project.python_environments.len(), 1);
        assert_eq!(
            project.python_environments[0].version.as_deref(),
            Some("3.12.4")
        );
        let python = tools(&report, "python");
        assert_eq!(python[0].source, "projectEnvironment");
        assert_eq!(python[0].label, ".venv (this project)");
        assert_eq!(python[0].version.as_deref(), Some("3.12.4"));
        assert_eq!(python[0].version_source, "pyvenv.cfg");
        assert_eq!(python[1].source, "path");
        assert_eq!(tools(&report, "jupyter")[0].source, "projectEnvironment");
    }

    #[test]
    fn missing_project_folders_are_reported_as_absent() {
        let report = detect_with(
            &host(Platform::Linux),
            Some(Path::new("/nonexistent/project")),
        );
        assert_eq!(report.project, None);
        assert!(report.boundary.contains("installed nothing"));
    }
}

#[test]
fn environment_identity_hashes_only_present_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("Manifest.toml"), "julia_version = \"1.10.4\"").unwrap();
    let identity = environment_identity(&root, &["Project.toml", "Manifest.toml"]).unwrap();
    assert_eq!(identity["files"].as_array().unwrap().len(), 1);
    assert_eq!(identity["files"][0]["path"], "Manifest.toml");
    assert_eq!(identity["checked"][0], "Project.toml");
}
