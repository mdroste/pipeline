//! Native presentation helpers, independent of Workspace and Workflow storage.
use std::path::Path;

/// The one check before any file goes to the system's default application, so
/// Files, Reviews, conversation links, and the external editor cannot drift apart.
/// A program must never be one click from running. `path` supplies the name and `executable` comes from
/// the file itself, because an extensionless program opens in a terminal and runs.
pub fn check_openable(path: &str, executable: bool) -> Result<(), &'static str> {
    if executable {
        return Err(
            "This file is marked as a program, so Pipeline won't open it. Reveal its folder \
             to open it yourself.",
        );
    }
    if runnable_by_system(path) {
        return Err(
            "Pipeline doesn't open this type of file in another app, because opening it could \
             run it. Reveal its folder to open it yourself.",
        );
    }
    Ok(())
}

/// Whether any execute permission bit is set. Windows has no such bits.
pub fn has_execute_bit(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        false
    }
}

/// Whether the system would run this file instead of showing it, whatever its
/// permission bits say.
pub fn runnable_by_system(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let name = name.trim_end_matches(['.', ' ']).to_ascii_lowercase();
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(
        extension,
        "app"
            | "appimage"
            | "appref-ms"
            | "application"
            | "appx"
            | "appxbundle"
            | "bash"
            | "bat"
            | "chm"
            | "cmd"
            | "com"
            | "command"
            | "cpl"
            | "csh"
            | "deb"
            | "desktop"
            | "diagcab"
            | "dmg"
            | "exe"
            | "fileloc"
            | "fish"
            | "gadget"
            | "hta"
            | "img"
            | "inetloc"
            | "inf"
            | "iso"
            | "jar"
            | "jse"
            | "ksh"
            | "library-ms"
            | "lnk"
            | "mobileconfig"
            | "mpkg"
            | "msc"
            | "msi"
            | "msix"
            | "msixbundle"
            | "msp"
            | "msu"
            | "pif"
            | "pkg"
            | "ps1"
            | "psm1"
            | "reg"
            | "rpm"
            | "scf"
            | "scpt"
            | "scptd"
            | "scr"
            | "sct"
            | "settingcontent-ms"
            | "sh"
            | "shb"
            | "shs"
            | "terminal"
            | "tool"
            | "url"
            | "vb"
            | "vbe"
            | "vbs"
            | "vhd"
            | "vhdx"
            | "webloc"
            | "workflow"
            | "ws"
            | "wsc"
            | "wsf"
            | "wsh"
            | "zsh"
    ) || (cfg!(windows) && extension == "js")
        // The python.org installers make these open by running them.
        || (cfg!(any(windows, target_os = "macos"))
            && matches!(extension, "py" | "pyc" | "pyo" | "pyw" | "pyz" | "pyzw"))
}

/// Forward-slash a canonical path and strip Windows verbatim prefixes
/// (`\\?\C:\…` → `C:/…`, `\\?\UNC\server\share` → `//server/share`), mirroring
/// `pipeline::claude::normalize_canonical_cli_path`. ShellExecute rejects
/// verbatim paths.
pub fn normalize_external_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix("//?/UNC/") {
        format!("//{rest}")
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        rest.to_string()
    } else {
        normalized
    }
}

/// A Windows path in the form ShellExecute and Explorer accept. A project root
/// is verbatim, and joining a relative path to one leaves its forward slashes.
#[cfg(any(windows, test))]
pub fn windows_system_path(path: &str) -> String {
    normalize_external_path(path).replace('/', "\\")
}

/// `path` as the system's opener and file manager need it.
pub fn system_path(path: &Path) -> std::path::PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        return windows_system_path(text).into();
    }
    path.to_path_buf()
}

pub fn reveal(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg("-R").arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(format!("/select,{}", system_path(path).display()));
        c
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path.parent().unwrap_or(path));
        c
    };
    let mut child = command
        .spawn()
        .map_err(|e| format!("Could not reveal file: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_openable, runnable_by_system, system_path, windows_system_path};
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Every way this crate can hand something to the system's default opener.
    const OPENERS: &[&str] = &[
        ".shell()",
        "::shell(",
        "Command::new(\"open\")",
        "Command::new(\"/usr/bin/open\")",
        "Command::new(\"xdg-open\")",
        "Command::new(\"explorer\")",
        "Command::new(\"explorer.exe\")",
    ];

    /// Non-comment production lines holding one of `patterns`, by file under `root`.
    fn sites(root: &Path, patterns: &[&str]) -> BTreeMap<String, usize> {
        fn visit(dir: &Path, root: &Path, patterns: &[&str], found: &mut BTreeMap<String, usize>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|name| name != "tests") {
                        visit(&path, root, patterns, found);
                    }
                    continue;
                }
                let name = path.file_name().unwrap().to_string_lossy();
                if !name.ends_with(".rs") || name.ends_with("tests.rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                // Test modules sit at the end of each file.
                let production = ["#[cfg(test)]\nmod ", "#[cfg(all(test, unix))]\nmod "]
                    .iter()
                    .fold(source.as_str(), |text, marker| {
                        text.split(marker).next().unwrap()
                    });
                let lines = production
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("//"))
                    .filter(|line| patterns.iter().any(|pattern| line.contains(pattern)))
                    .count();
                if lines > 0 {
                    let relative = path.strip_prefix(root).unwrap();
                    found.insert(relative.to_string_lossy().replace('\\', "/"), lines);
                }
            }
        }
        let mut found = BTreeMap::new();
        visit(root, root, patterns, &mut found);
        found
    }

    fn source_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    /// A new way to reach the system's opener has to be listed here with what
    /// makes it safe. One that opens a file has to pass `check_openable` first.
    #[test]
    fn every_route_to_the_system_opener_is_accounted_for() {
        let accounted: BTreeMap<String, usize> = [
            // `is_safe_auth_url` admits https addresses only.
            ("agent_runtime/codex/chatgpt/commands.rs", 1),
            // `check_artifact`.
            ("commands/artifacts.rs", 1),
            // The data folder, resolved here and never passed in.
            ("commands/config.rs", 3),
            // Reveals a package this session exported, and opens an HTML report
            // this function just wrote.
            ("commands/export/desktop.rs", 6),
            // `reveal` shows a file in its folder and never opens it.
            ("file_viewer.rs", 3),
            // `external_target`, `snapshot_conversation_file`, and
            // `editor_external_path`.
            ("workbench/commands/project.rs", 3),
        ]
        .into_iter()
        .map(|(path, count)| (path.to_string(), count))
        .collect();
        assert_eq!(sites(&source_root(), OPENERS), accounted);
    }

    #[test]
    fn each_file_route_to_the_opener_still_makes_the_check() {
        let calls: BTreeMap<String, usize> = [
            "commands/artifacts.rs",
            "workbench/project/file_browser.rs",
            "workbench/project/file_workspace.rs",
            "workbench/project/studio/manuscript.rs",
        ]
        .into_iter()
        .map(|path| (path.to_string(), 1))
        .collect();
        assert_eq!(
            sites(&source_root(), &["file_viewer::check_openable("]),
            calls
        );
    }

    #[test]
    fn the_scan_finds_a_new_opener_and_skips_tests_and_comments() {
        let root = tempfile::tempdir().unwrap();
        let write = |name: &str, text: &str| {
            let path = root.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "feature/new.rs",
            "fn a() {\n    app.shell()\n        .open(path, None);\n}\n",
        );
        write(
            "feature/spawned.rs",
            "fn b() { Command::new(\"xdg-open\").spawn(); }\n",
        );
        write("feature/noted.rs", "// app.shell().open(path, None);\n");
        write("feature/checked_tests.rs", "fn c() { app.shell(); }\n");
        write("tests/helper.rs", "fn d() { app.shell(); }\n");
        write(
            "feature/tested.rs",
            "fn e() {}\n#[cfg(test)]\nmod tests {\n    fn f() { app.shell(); }\n}\n",
        );
        let found: Vec<(String, usize)> = sites(root.path(), OPENERS).into_iter().collect();
        assert_eq!(
            found,
            [
                ("feature/new.rs".to_string(), 1),
                ("feature/spawned.rs".to_string(), 1),
            ]
        );
    }

    #[test]
    fn scripts_a_platform_would_run_are_recognized() {
        for path in [
            "macro.vb",
            "explorer.scf",
            "help.chm",
            "update.msu",
            "setup.appx",
            "profile.mobileconfig",
            "help.sct",
            "script.wsc",
            "scrap.shb",
            "scrap.shs",
            "driver.inf",
            "setup.appxbundle",
            "setup.msixbundle",
            "fix.diagcab",
            "panel.settingcontent-ms",
            "folder.library-ms",
            "disk.iso",
            "disk.img",
            "disk.vhd",
            "disk.vhdx",
        ] {
            assert!(runnable_by_system(path), "{path}");
        }
        for path in [
            "analysis.py",
            "gui.pyw",
            "cache.pyc",
            "cache.pyo",
            "app.pyz",
            "gui.pyzw",
        ] {
            assert_eq!(
                runnable_by_system(path),
                cfg!(any(windows, target_os = "macos")),
                "{path}"
            );
        }
    }

    #[test]
    fn system_paths_lose_their_verbatim_prefixes_and_keep_their_separators() {
        for (path, expected) in [
            (
                r"\\?\C:\Users\Mike\paper\main.tex",
                r"C:\Users\Mike\paper\main.tex",
            ),
            (
                r"\\?\C:\Users\Mike\paper\sub/main.tex",
                r"C:\Users\Mike\paper\sub\main.tex",
            ),
            (
                r"\\?\UNC\server\share\sub/main.tex",
                r"\\server\share\sub\main.tex",
            ),
            (r"C:\Users\Mike\main.tex", r"C:\Users\Mike\main.tex"),
        ] {
            assert_eq!(windows_system_path(path), expected);
        }
        #[cfg(unix)]
        assert_eq!(system_path(Path::new("/a/b.txt")), Path::new("/a/b.txt"));
    }

    #[test]
    fn opening_refuses_a_program_by_its_name_or_its_permission_bits() {
        assert!(check_openable("paper.pdf", false).is_ok());
        assert!(check_openable("results", false).is_ok());
        let marked = check_openable("results", true).unwrap_err();
        let by_type = check_openable("run.bat", false).unwrap_err();
        assert_eq!(check_openable("setup.jar", false).unwrap_err(), by_type);
        // Each action reaches these, and most of them don't ask to run anything.
        for refusal in [marked, by_type] {
            assert!(refusal.contains("Reveal its folder"), "{refusal}");
            assert!(
                !refusal.to_lowercase().contains("will not run"),
                "{refusal}"
            );
        }
    }

    #[test]
    fn programs_are_recognized_in_any_case_and_with_trailing_dots() {
        for path in [
            "run.command",
            "a/b/Setup.EXE",
            "evil.bat.",
            "share.webloc",
            "launch.sh",
            "notes.pdf.app",
        ] {
            assert!(runnable_by_system(path), "{path}");
        }
        for path in ["paper.pdf", "figure.png", "main.tex", "command", "data.csv"] {
            assert!(!runnable_by_system(path), "{path}");
        }
    }
}
