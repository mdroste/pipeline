//! Converter discovery and argv policy. No source-provided command is executed.
//! macOS/Linux also confine filesystem reads at the OS boundary. Windows uses
//! installed native TeX with shell escape disabled; engine flags are not an OS
//! filesystem sandbox. No automatic WSL or package-installation fallback.
use super::*;
use crate::deps::ResolvedCommand;
use std::ffi::{OsStr, OsString};

pub(super) fn libreoffice() -> Option<ResolvedCommand> {
    for name in ["soffice", "libreoffice"] {
        if let Some(command) = crate::deps::resolve_command(name) {
            return Some(command);
        }
    }
    let mut candidates = vec![PathBuf::from(
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    )];
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join("LibreOffice/program/soffice.exe"));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(ResolvedCommand::direct)
}

pub(super) fn office_profile(path: &Path) -> Result<(), String> {
    let user = path.join("user");
    fs::create_dir_all(&user).map_err(|e| e.to_string())?;
    fs::write(user.join("registrymodifications.xcu"), r#"<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry">
<item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop><prop oor:name="DisableMacrosExecution" oor:op="fuse"><value>true</value></prop></item>
<item oor:path="/org.openoffice.Office.Common/Load"><prop oor:name="UpdateLink" oor:op="fuse"><value>0</value></prop></item>
</oor:items>"#).map_err(|e| e.to_string())
}

pub(super) fn office_args(
    input: &Path,
    output: &Path,
    profile: &Path,
) -> Result<Vec<OsString>, String> {
    let profile = reqwest::Url::from_directory_path(profile)
        .map_err(|_| "Invalid LibreOffice profile path")?;
    Ok(vec![
        format!("-env:UserInstallation={profile}").into(),
        "--headless".into(), "--nologo".into(), "--nodefault".into(), "--norestore".into(),
        "--convert-to".into(),
        // Match native extraction, which includes hidden slides but not notes pages.
        r#"pdf:impress_pdf_Export:{"ExportHiddenSlides":{"type":"boolean","value":"true"},"ExportNotesPages":{"type":"boolean","value":"false"}}"#.into(),
        "--outdir".into(), output.as_os_str().into(), input.as_os_str().into(),
    ])
}

pub(super) fn tex_engine(source: &str) -> Result<String, String> {
    let header: String = source.lines().take(30).collect::<Vec<_>>().join("\n");
    let directive = Regex::new(r"(?im)^\s*%\s*!\s*tex\s+(?:ts-)?program\s*=\s*([^\s]+)").unwrap();
    if let Some(capture) = directive.captures(&header) {
        let engine = capture[1].to_ascii_lowercase();
        if matches!(engine.as_str(), "pdflatex" | "xelatex" | "lualatex") {
            return Ok(engine);
        }
        return Err(format!(
            "Unsupported TeX engine '{engine}'; use pdflatex, xelatex, or lualatex"
        ));
    }
    let visible = latex_without_comments(source);
    let unicode =
        Regex::new(r"\\usepackage(?:\s*\[[^\]]*\])?\s*\{[^}]*\b(?:fontspec|unicode-math)\b")
            .unwrap();
    Ok(if unicode.is_match(&visible) {
        "xelatex"
    } else {
        "pdflatex"
    }
    .into())
}

#[cfg(not(windows))]
pub(super) fn tex_command(engine: &str) -> Option<ResolvedCommand> {
    crate::deps::resolve_command(engine).or_else(|| {
        let path = Path::new("/Library/TeX/texbin").join(engine);
        path.is_file().then(|| ResolvedCommand::direct(path))
    })
}

pub(super) fn tex_args(engine: &str, filename: &OsStr, output: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-no-shell-escape",
        "-interaction=nonstopmode",
        "-halt-on-error",
        "-file-line-error",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    if engine == "lualatex" {
        args.push("--nosocket".into());
    }
    args.push(format!("-output-directory={}", output.display()).into());
    // Prefixing with ./ prevents a leading '-' in the selected filename from
    // becoming an engine option. Every argument remains an OS argument.
    args.push(Path::new(".").join(filename).into_os_string());
    args
}

pub(super) fn tex_environment(command: &mut StdCommand, workspace: &Path, output: &Path) {
    command
        .env_clear()
        .env("PATH", env::full_path())
        .env("LANG", "C.UTF-8")
        .env("HOME", workspace)
        .env("TMPDIR", workspace.join("tmp"))
        .env("TEXMFHOME", workspace.join("texmf-home"))
        .env("TEXMFVAR", workspace.join("texmf-var"))
        .env("TEXMFCONFIG", workspace.join("texmf-config"))
        .env("TEXMFOUTPUT", output)
        // LuaLaTeX's font loader needs absolute paths to system fonts. This is
        // not a filesystem security boundary; only macOS/Linux add an OS sandbox.
        .env("openin_any", "a")
        .env("openout_any", "p")
        .env("shell_escape", "f");
    #[cfg(windows)]
    super::windows::environment(command, workspace);
}

#[cfg(not(windows))]
pub(super) fn confined_tex_command(
    program: &ResolvedCommand,
    args: &[OsString],
    workspace: &Path,
) -> Result<StdCommand, String> {
    let workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
    fs::create_dir_all(workspace.join("tmp")).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        let profile = macos_profile(&workspace, program.program())?;
        let mut command = StdCommand::new("/usr/bin/sandbox-exec");
        command
            .args(["-p", &profile, "--"])
            .arg(program.program())
            .args(program.prefix_args())
            .args(args);
        Ok(command)
    }
    #[cfg(target_os = "linux")]
    {
        let sandbox = crate::deps::resolve_command("bwrap")
            .ok_or("Install bubblewrap to enable isolated Beamer compilation")?;
        let mut command = sandbox.command([
            "--die-with-parent",
            "--unshare-all",
            "--new-session",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ]);
        for root in [
            "/usr",
            "/bin",
            "/sbin",
            "/lib",
            "/lib64",
            "/etc",
            "/opt",
            "/var/lib/texmf",
            "/var/cache/fontconfig",
        ] {
            if Path::new(root).exists() {
                command.args(["--ro-bind", root, root]);
            }
        }
        command.arg("--bind").arg(&workspace).arg(&workspace);
        command
            .arg("--")
            .arg(program.program())
            .args(program.prefix_args())
            .args(args);
        Ok(command)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (program, args, workspace);
        Err("Automatic Beamer compilation is unavailable on this operating system".into())
    }
}

#[cfg(target_os = "macos")]
fn macos_profile(workspace: &Path, executable: &Path) -> Result<String, String> {
    fn quoted(path: &Path) -> Result<String, String> {
        let text = path.to_str().ok_or("TeX sandbox path must be UTF-8")?;
        if text.chars().any(char::is_control) {
            return Err("Invalid control character in TeX sandbox path".into());
        }
        Ok(format!(
            "\"{}\"",
            text.replace('\\', "\\\\").replace('"', "\\\"")
        ))
    }
    // dyld enumerates the filesystem root during startup on recent macOS.
    // A literal directory grant exposes its entries, not descendants' data.
    let mut profile = String::from("(version 1)\n(deny default)\n(allow process* sysctl-read mach-lookup signal file-read-metadata)\n(allow file-read-data (literal \"/\"))\n(allow file-read* (literal \"/dev/null\") (literal \"/dev/urandom\") (literal \"/dev/random\"))\n(allow file-write* (literal \"/dev/null\"))\n");
    for root in [
        "/System",
        "/Library",
        "/usr",
        "/bin",
        "/sbin",
        "/opt",
        "/private/etc",
    ] {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))\n",
            quoted(Path::new(root))?
        ));
    }
    if let Some(parent) = executable.parent() {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))\n",
            quoted(parent)?
        ));
    }
    if let Some(home) = dirs::home_dir() {
        profile.push_str(&format!(
            "(allow file-read* (subpath {}))\n",
            quoted(&home.join("Library/Fonts"))?
        ));
    }
    profile.push_str(&format!(
        "(allow file-read* file-write* (subpath {}))\n",
        quoted(workspace)?
    ));
    Ok(profile)
}
