//! Native Windows TeX discovery and invocation. Windows compilation deliberately
//! uses the installed local engine, not WSL or an installation fallback. It uses
//! the shared bounded process runner and private build copy, disables shell
//! escape, and prevents MiKTeX from installing packages or opening prompts.
//! Unlike the macOS/Linux route, this is not an OS filesystem sandbox.
use super::*;
use crate::deps::ResolvedCommand;
use std::ffi::OsString;

#[cfg(windows)]
pub(super) fn required_engine(engine: &str) -> Result<ResolvedCommand, String> {
    if !matches!(engine, "pdflatex" | "xelatex" | "lualatex") {
        return Err(format!("Unsupported native Windows TeX engine: {engine}"));
    }
    let mut roots = Vec::new();
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(root).join("Programs/MiKTeX"));
    }
    for name in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(name) {
            roots.push(PathBuf::from(root).join("MiKTeX"));
        }
    }
    let drive = std::env::var_os("SystemDrive").unwrap_or_else(|| "C:".into());
    let texlive = PathBuf::from(format!("{}\\texlive", drive.to_string_lossy()));
    select_engine(
        engine,
        crate::deps::resolve_command(engine),
        candidates(engine, &roots, &texlive),
    )
}

fn select_engine(
    engine: &str,
    on_path: Option<ResolvedCommand>,
    candidates: Vec<PathBuf>,
) -> Result<ResolvedCommand, String> {
    if let Some(command) = on_path.filter(native_executable) {
        return Ok(command);
    }
    candidates.into_iter().find(|path| path.is_file())
        .map(ResolvedCommand::direct)
        .ok_or_else(|| format!("Native Windows TeX engine '{engine}.exe' was not found. Install TeX Live or MiKTeX with Beamer, or add its binary directory to PATH. Automatic compilation does not use WSL."))
}

fn native_executable(command: &ResolvedCommand) -> bool {
    command.prefix_args().is_empty()
        && command
            .program()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

fn candidates(engine: &str, miktex_roots: &[PathBuf], texlive: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for root in miktex_roots {
        for bin in ["miktex/bin/x64", "miktex/bin"] {
            paths.push(root.join(bin).join(format!("{engine}.exe")));
        }
    }
    // Only conventional year directories, with a bounded shallow scan. Custom
    // and portable installations are discoverable through PATH.
    let mut years: Vec<_> = fs::read_dir(texlive)
        .into_iter()
        .flatten()
        .take(64)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.len() == 4 && name.bytes().all(|b| b.is_ascii_digit())
        })
        .map(|entry| entry.path())
        .collect();
    years.sort();
    for root in years.into_iter().rev() {
        for bin in ["bin/windows", "bin/win32"] {
            paths.push(root.join(bin).join(format!("{engine}.exe")));
        }
    }
    paths
}

#[cfg(windows)]
pub(super) fn is_miktex(
    program: &ResolvedCommand,
    workspace: &Path,
    started: Instant,
) -> Result<bool, String> {
    crate::pipeline::review_budget::check()?;
    if crate::cancellation::is_cancelled() {
        return Err("Pipeline cancelled".into());
    }
    let remaining = RENDER_TIMEOUT
        .checked_sub(started.elapsed())
        .ok_or("Slide rendering exceeded its two-minute time limit")?;
    let mut command = program.command(["--version"]);
    tools::tex_environment(&mut command, workspace, &workspace.join("output"));
    let result = run_bounded_output(
        command,
        "Native Windows TeX version",
        remaining.min(Duration::from_secs(10)),
        16 * 1024,
        None,
    )?;
    if !result.status.success() || result.stdout_truncated {
        return Err("Could not identify the installed native Windows TeX engine".into());
    }
    identify(&String::from_utf8_lossy(&result.stdout))
}

fn identify(version: &str) -> Result<bool, String> {
    if version.contains("MiKTeX") {
        Ok(true)
    } else if version.contains("TeX Live") {
        Ok(false)
    } else {
        Err("Automatic Windows compilation requires native TeX Live or MiKTeX".into())
    }
}

pub(super) fn command(
    program: &ResolvedCommand,
    mut args: Vec<OsString>,
    miktex: bool,
) -> StdCommand {
    // Keep the filename last. Source text never enters a shell or command string.
    let filename = args.pop().expect("TeX invocation includes the main file");
    if miktex {
        args.retain(|arg| arg != "-no-shell-escape");
        args.extend(
            [
                "--disable-write18",
                "--disable-installer",
                "--dont-parse-first-line",
            ]
            .map(OsString::from),
        );
    } else {
        args.push("-no-parse-first-line".into());
    }
    args.push(filename);
    program.command(args)
}

#[cfg(windows)]
pub(super) fn environment(command: &mut StdCommand, workspace: &Path) {
    // Windows' loader and MiKTeX need the OS and per-user configuration paths.
    // Keep only those explicit values from the caller's environment; compiler
    // temporaries and aux files still go to the private build/output directories.
    for name in [
        "SystemRoot",
        "WINDIR",
        "SystemDrive",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("TEMP", workspace.join("tmp"))
        .env("TMP", workspace.join("tmp"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_prefers_current_texlive_and_keeps_paths_with_spaces() {
        let root = tempfile::tempdir().unwrap();
        for name in ["2025", "2026", "unrelated"] {
            fs::create_dir(root.path().join(name)).unwrap();
        }
        let miktex = root.path().join("Program Files/MiKTeX");
        let paths = candidates("xelatex", std::slice::from_ref(&miktex), root.path());
        assert_eq!(paths[0], miktex.join("miktex/bin/x64/xelatex.exe"));
        assert_eq!(paths[2], root.path().join("2026/bin/windows/xelatex.exe"));
        assert!(!paths
            .iter()
            .any(|p| p.to_string_lossy().contains("unrelated")));
        assert!(native_executable(&ResolvedCommand::direct(
            paths[0].clone()
        )));
        assert!(!native_executable(&ResolvedCommand::direct(
            root.path().join("pdflatex.cmd")
        )));
    }

    #[test]
    fn an_absent_engine_is_an_error_and_shell_shims_do_not_count() {
        let error = select_engine("pdflatex", None, Vec::new()).unwrap_err();
        assert!(error.contains("Native Windows TeX"));
        assert!(error.contains("was not found"));
        let shim = ResolvedCommand::direct(PathBuf::from("pdflatex.cmd"));
        assert!(select_engine("pdflatex", Some(shim), Vec::new()).is_err());
        let native = ResolvedCommand::direct(PathBuf::from("pdflatex.exe"));
        assert!(select_engine("pdflatex", Some(native), Vec::new()).is_ok());
    }

    #[test]
    fn native_commands_disable_shells_installers_and_source_directives() {
        let program = ResolvedCommand::direct(PathBuf::from("C:/TeX tools/pdflatex.exe"));
        let output = Path::new("C:/build with spaces/output");
        let args = tools::tex_args(
            "pdflatex",
            std::ffi::OsStr::new("-talk & results.tex"),
            output,
        );
        for miktex in [false, true] {
            let command = command(&program, args.clone(), miktex);
            let argv: Vec<_> = command.get_args().collect();
            assert_eq!(command.get_program(), program.program());
            assert_eq!(argv.last().unwrap(), &args.last().unwrap().as_os_str());
            if miktex {
                assert!(argv.contains(&std::ffi::OsStr::new("--disable-write18")));
                assert!(argv.contains(&std::ffi::OsStr::new("--disable-installer")));
            } else {
                assert!(argv.contains(&std::ffi::OsStr::new("-no-shell-escape")));
                assert!(argv.contains(&std::ffi::OsStr::new("-no-parse-first-line")));
            }
        }
        assert!(!identify("pdfTeX (TeX Live 2026)").unwrap());
        assert!(identify("MiKTeX-pdfTeX 4.21 (MiKTeX 25.4)").unwrap());
        assert!(identify("unrecognized executable").is_err());
    }
}
