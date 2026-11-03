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

/// Whether the file behind `file`, named by `path`, is a program as far as
/// `check_openable` goes. Windows has no execute bits. A volume that keeps them
/// is taken at its word. On one that invents them every file looks like a
/// program, so the contents decide, and a name with no extension still counts,
/// because macOS treats such a file as a program there and runs it in a terminal.
pub fn marked_as_program(path: &str, file: &std::fs::File) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileExt as _, PermissionsExt as _};
        if file.metadata()?.permissions().mode() & 0o111 == 0 {
            return Ok(false);
        }
        if crate::safety::volume_keeps_permissions(file) {
            return Ok(true);
        }
        let mut start = [0; 4];
        let mut filled = 0;
        while filled < start.len() {
            match file.read_at(&mut start[filled..], filled as u64)? {
                0 => break,
                count => filled += count,
            }
        }
        Ok(!has_extension(path) || starts_like_program(&start[..filled]))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, file);
        Ok(false)
    }
}

/// What macOS counts as an extension: a last dotted part with no space, after a
/// name that is more than dots.
#[cfg(any(unix, test))]
fn has_extension(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.').is_some_and(|(stem, extension)| {
        !extension.is_empty() && !extension.contains(' ') && !stem.trim_matches('.').is_empty()
    })
}

/// Mach-O (either byte order, and universal), ELF, Windows PE, and `#!` scripts.
#[cfg(any(unix, test))]
fn starts_like_program(start: &[u8]) -> bool {
    start.starts_with(b"#!")
        || start.starts_with(b"MZ")
        || matches!(
            start,
            [0x7f, b'E', b'L', b'F']
                | [0xfe, 0xed, 0xfa, 0xce | 0xcf]
                | [0xce | 0xcf, 0xfa, 0xed, 0xfe]
                | [0xca, 0xfe, 0xba, 0xbe | 0xbf]
                | [0xbe | 0xbf, 0xba, 0xfe, 0xca]
        )
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

/// Explorer reads `/select,"C:\\a b\\c.tex"` as one option. Rust's own quoting
/// would wrap the option too, and Explorer then opens Documents. A Windows file
/// name can't hold a quote, so this quoting is complete.
#[cfg(any(windows, test))]
fn select_argument(path: &str) -> String {
    format!("/select,\"{path}\"")
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
        use std::os::windows::process::CommandExt as _;
        c.raw_arg(select_argument(&system_path(path).to_string_lossy()));
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
    use super::{
        check_openable, has_extension, runnable_by_system, select_argument, starts_like_program,
        windows_system_path,
    };
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
        assert_eq!(
            super::system_path(Path::new("/a/b.txt")),
            Path::new("/a/b.txt")
        );
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
    fn explorer_gets_one_selection_with_only_the_path_quoted() {
        assert_eq!(
            select_argument(r"C:\Users\Ada Lovelace\paper\main.tex"),
            r#"/select,"C:\Users\Ada Lovelace\paper\main.tex""#
        );
    }

    /// Names as macOS's own type lookup read them on an exFAT volume.
    #[test]
    fn a_name_has_an_extension_where_the_system_finds_one() {
        for path in [
            "main.tex",
            "paper/report.2024",
            "two.parts.q9",
            "a.b-c",
            "a.b_c",
            "a.\u{4e2d}\u{6587}",
        ] {
            assert!(has_extension(path), "{path}");
        }
        for path in [
            "results",
            "paper/results",
            "a.b/results",
            ".bashrc",
            "..x",
            "v1. final",
            "a.b c",
            "trail.",
            "a. ",
        ] {
            assert!(!has_extension(path), "{path}");
        }
    }

    #[test]
    fn the_start_of_a_file_shows_each_kind_of_program() {
        for start in [
            b"#!/b".as_slice(),
            b"#!",
            b"MZ\x90\0",
            b"\x7fELF",
            b"\xfe\xed\xfa\xce",
            b"\xfe\xed\xfa\xcf",
            b"\xce\xfa\xed\xfe",
            b"\xcf\xfa\xed\xfe",
            b"\xca\xfe\xba\xbe",
            b"\xca\xfe\xba\xbf",
            b"\xbe\xba\xfe\xca",
        ] {
            assert!(starts_like_program(start), "{start:?}");
        }
        for start in [
            b"%PDF".as_slice(),
            b"\\doc",
            b"PK\x03\x04",
            b"\x89PNG",
            b"",
            b"#",
            b"a,b\n",
        ] {
            assert!(!starts_like_program(start), "{start:?}");
        }
    }

    /// Set `TMPDIR` to a folder on an exFAT drive to run this on one: the
    /// ordinary files there carry the bit, and only the program is refused.
    #[cfg(unix)]
    #[test]
    fn this_volume_refuses_real_programs_and_opens_ordinary_files() {
        use super::marked_as_program;
        use std::os::unix::fs::PermissionsExt as _;
        let temp = tempfile::tempdir().unwrap();
        let write = |name: &str, bytes: &[u8], mode: u32| {
            let path = temp.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            std::fs::File::open(path).unwrap()
        };
        let marked = |name: &str, file: &std::fs::File| marked_as_program(name, file).unwrap();

        let plain = write("paper.pdf", b"%PDF-1.7", 0o644);
        let keeps = crate::safety::volume_keeps_permissions(&plain);
        assert!(!marked("paper.pdf", &plain));
        let notes = write("notes.txt", b"a note", 0o755);
        assert_eq!(marked("notes.txt", &notes), keeps);

        let program = std::fs::read("/bin/sh").unwrap();
        let named = write("table.csv", &program, 0o755);
        assert!(marked("table.csv", &named));
        let script = write("report.dat", b"#!/bin/sh\n", 0o755);
        assert!(marked("report.dat", &script));
        let bare = write("results", b"rm -rf output\n", 0o755);
        assert!(marked("results", &bare));
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
