//! Adapter catalog and argument-shape validation for host execution profiles.
//!
//! Each adapter accepts one documented argv shape, so output checks, stop
//! handling and environment capture know which program runs and which project
//! file it reads. Arguments go directly to the program without a shell. The
//! one exception is the Stata custom launcher, which deliberately runs a shell
//! alias and therefore accepts only plain words and quoted paths.

use super::profiles::safe_relative_path;
use crate::workbench::store::{WorkbenchError, WorkbenchResult};

/// Adapters accepted when a profile is saved. `command` keeps its original,
/// unconstrained behavior for compatibility.
pub(super) const ADAPTERS: [&str; 8] = [
    "command", "latex", "stata", "r", "python", "julia", "notebook", "quarto",
];

const ARGUMENT_OPERATORS: [char; 9] = [';', '&', '|', '`', '$', '<', '>', '\n', '\r'];
const LAUNCHER_OPERATORS: [char; 10] = [';', '&', '|', '`', '$', '<', '>', '\n', '\r', '!'];

const STATA_SHAPE: &str = "Stata profiles run an installed Stata in batch mode: stata-mp -b do main.do on macOS and Linux, or StataMP-64.exe /e do main.do on Windows. To use a shell alias instead, enter /bin/zsh -lic \"stata -q -b do main.do\" as a custom launcher.";
const LAUNCHER_SHAPE: &str = "A Stata custom launcher must be /bin/zsh -lic (or /bin/bash -lic) followed by one command such as \"stata -q -b do main.do\": a shell alias, -b, do, and the do-file, with no shell operators.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StataMode {
    /// The installed Stata executable, started directly in batch mode.
    Direct,
    /// A shell alias or wrapper started through `zsh -lic` or `bash -lic`.
    CustomLauncher,
}

/// Facts about a validated argv that launch, outputs and receipts rely on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AdapterShape {
    /// Project-relative script or document the program reads.
    pub script: Option<String>,
    /// Output the program writes that must be declared (nbconvert copies).
    pub required_output: Option<String>,
    /// Stata batch log file name, written in the working folder.
    pub stata_log: Option<String>,
}

pub(super) fn check_adapter_name(adapter: &str) -> WorkbenchResult<()> {
    if ADAPTERS.contains(&adapter) {
        Ok(())
    } else {
        Err(WorkbenchError::invalid(
            "Execution adapter must be command, latex, stata, r, python, julia, notebook, or quarto",
        ))
    }
}

/// Lowercase executable file name, without `.exe` on Windows.
pub(super) fn program_stem(program: &str, windows: bool) -> String {
    let name = file_name(program, windows).to_ascii_lowercase();
    if windows {
        name.strip_suffix(".exe").map(str::to_owned).unwrap_or(name)
    } else {
        name
    }
}

fn file_name(path: &str, windows: bool) -> &str {
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    path.rsplit(separators).next().unwrap_or(path)
}

pub(super) fn is_stata_program(stem: &str) -> bool {
    let stem = stem.strip_suffix("-64").unwrap_or(stem);
    matches!(
        stem,
        "stata"
            | "stata-mp"
            | "stata-se"
            | "stata-be"
            | "stata-ic"
            | "statamp"
            | "statase"
            | "statabe"
            | "stataic"
    )
}

fn is_launcher_shell(program: &str) -> bool {
    program.starts_with('/') && matches!(file_name(program, false), "zsh" | "bash")
}

fn is_shell(program: &str, windows: bool) -> bool {
    matches!(
        program_stem(program, windows).as_str(),
        "sh" | "zsh" | "bash" | "dash" | "ksh"
    )
}

pub(super) fn stata_mode(argv: &[String]) -> StataMode {
    if argv.len() == 3 && is_launcher_shell(&argv[0]) {
        StataMode::CustomLauncher
    } else {
        StataMode::Direct
    }
}

/// Custom launchers may restore machine state on a normal return (for example
/// a clock wrapper), so a stop request waits briefly before forcing them.
pub(super) fn waits_for_stata_launcher(adapter: &str, argv: &[String]) -> bool {
    adapter == "stata" && stata_mode(argv) == StataMode::CustomLauncher
}

pub(super) fn stata_mode_label(adapter: &str, argv: &[String]) -> Option<&'static str> {
    (adapter == "stata").then(|| match stata_mode(argv) {
        StataMode::Direct => "direct",
        StataMode::CustomLauncher => "customLauncher",
    })
}

/// Validate the argv shape of a profile being saved.
pub(super) fn validate_adapter_argv(
    adapter: &str,
    argv: &[String],
    outputs: &[String],
    windows: bool,
) -> WorkbenchResult<()> {
    check_adapter_name(adapter)?;
    if argv.is_empty() {
        return Err(WorkbenchError::invalid(
            "Execution argv must name a program",
        ));
    }
    if matches!(adapter, "command" | "latex") {
        if routes_stata(argv, windows) {
            return Err(WorkbenchError::invalid(
                "Run Stata with the Stata adapter so Pipeline checks its log and handles stopping.",
            ));
        }
        return Ok(());
    }
    let shape = parse_adapter(adapter, argv, windows).map_err(WorkbenchError::invalid)?;
    if let Some(log) = &shape.stata_log {
        if !outputs
            .iter()
            .any(|output| names_match(file_name(output, windows), log, windows))
        {
            return Err(WorkbenchError::invalid(format!(
                "Add {log} to the expected outputs so Pipeline can check the Stata log for errors."
            )));
        }
    }
    if let Some(required) = &shape.required_output {
        if !outputs
            .iter()
            .any(|output| same_relative(output, required, windows))
        {
            return Err(WorkbenchError::invalid(format!(
                "Add {required} to the expected outputs; it is the executed copy of the notebook."
            )));
        }
    }
    if adapter == "quarto" && outputs.is_empty() {
        return Err(WorkbenchError::invalid(
            "Add the rendered document, such as report.html, to the expected outputs.",
        ));
    }
    Ok(())
}

/// The project file a structured adapter reads, when it has one.
pub(super) fn launch_script(adapter: &str, argv: &[String], windows: bool) -> Option<String> {
    if matches!(adapter, "command" | "latex") {
        return None;
    }
    parse_adapter(adapter, argv, windows).ok()?.script
}

/// Extra launch-file extensions hashed for document adapters. Existing
/// adapters keep their original list so their authorizations stay valid.
pub(super) fn launch_file_extensions(adapter: &str) -> &'static [&'static str] {
    match adapter {
        "notebook" => &["ipynb"],
        "quarto" => &["qmd", "rmd", "ipynb"],
        _ => &[],
    }
}

/// Working-folder files whose hashes identify a structured adapter's
/// environment. `None` keeps a profile's launch identity unchanged.
pub(super) fn environment_files(adapter: &str, argv: &[String]) -> Option<&'static [&'static str]> {
    const PYTHON: &[&str] = &[
        "uv.lock",
        "requirements.txt",
        "environment.yml",
        "environment.yaml",
        "pyproject.toml",
        "poetry.lock",
        "Pipfile.lock",
        ".python-version",
    ];
    match adapter {
        "r" => Some(&["renv.lock", ".Rprofile", "renv/activate.R", "DESCRIPTION"]),
        "python" | "notebook" => Some(PYTHON),
        "julia" => Some(&[
            "Project.toml",
            "Manifest.toml",
            "JuliaProject.toml",
            "JuliaManifest.toml",
        ]),
        "quarto" => Some(&[
            "_quarto.yml",
            "_quarto.yaml",
            "renv.lock",
            "uv.lock",
            "requirements.txt",
            "environment.yml",
            "Project.toml",
            "Manifest.toml",
        ]),
        "stata" if stata_mode(argv) == StataMode::Direct => Some(&["profile.do"]),
        _ => None,
    }
}

pub(super) fn parse_adapter(
    adapter: &str,
    argv: &[String],
    windows: bool,
) -> Result<AdapterShape, String> {
    if argv.is_empty() {
        return Err("Execution argv must name a program".into());
    }
    match adapter {
        "stata" => match stata_mode(argv) {
            StataMode::Direct => parse_stata_direct(argv, windows),
            StataMode::CustomLauncher if !windows => parse_stata_launcher(argv),
            StataMode::CustomLauncher => Err(STATA_SHAPE.into()),
        },
        "r" => parse_script(argv, windows, &R_SCRIPT),
        "python" => parse_script(argv, windows, &PYTHON_SCRIPT),
        "julia" => parse_script(argv, windows, &JULIA_SCRIPT),
        "notebook" => parse_notebook(argv, windows),
        "quarto" => parse_quarto(argv, windows),
        _ => Ok(AdapterShape::default()),
    }
}

fn routes_stata(argv: &[String], windows: bool) -> bool {
    is_stata_program(&program_stem(&argv[0], windows))
        || argv
            .iter()
            .any(|argument| argument.trim_start().starts_with("oldstata "))
        || (is_shell(&argv[0], windows)
            && argv[1..].iter().any(|argument| {
                argument.split_whitespace().next().is_some_and(|word| {
                    word == "oldstata" || is_stata_program(&word.to_ascii_lowercase())
                })
            }))
}

fn reject_operators(argv: &[String]) -> Result<(), String> {
    if argv
        .iter()
        .any(|argument| argument.contains(ARGUMENT_OPERATORS))
    {
        return Err("Arguments cannot contain shell operators such as ; & | ` $ < >. This adapter passes each argument directly to the program.".into());
    }
    Ok(())
}

fn names_match(left: &str, right: &str, windows: bool) -> bool {
    if windows {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

fn same_relative(left: &str, right: &str, windows: bool) -> bool {
    if windows {
        left.replace('\\', "/")
            .eq_ignore_ascii_case(&right.replace('\\', "/"))
    } else {
        left == right
    }
}

fn extension(path: &str, windows: bool) -> Option<String> {
    let name = file_name(path, windows);
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| extension.to_ascii_lowercase())
}

fn project_file(label: &str, path: &str, extensions: &[&str], windows: bool) -> Result<(), String> {
    safe_relative_path(label, path).map_err(|_| {
        format!(
            "{label} file {path} must be a path inside the working folder, such as code/{}",
            example_name(extensions)
        )
    })?;
    if windows && (path.contains(':') || path.starts_with('\\')) {
        return Err(format!(
            "{label} file {path} must be relative to the working folder"
        ));
    }
    if !extension(path, windows).is_some_and(|value| extensions.contains(&value.as_str())) {
        let listed = extensions
            .iter()
            .map(|value| format!(".{value}"))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("{label} file {path} must end in {listed}"));
    }
    Ok(())
}

fn example_name(extensions: &[&str]) -> String {
    format!("analysis.{}", extensions.first().copied().unwrap_or("txt"))
}

struct ScriptSpec {
    label: &'static str,
    program: fn(&str) -> bool,
    example: &'static str,
    extensions: &'static [&'static str],
    flags: &'static [&'static str],
    flag_prefixes: &'static [&'static str],
}

fn is_rscript(stem: &str) -> bool {
    stem == "rscript"
}

fn is_python(stem: &str) -> bool {
    stem == "python"
        || stem == "python3"
        || stem.strip_prefix("python3.").is_some_and(|minor| {
            !minor.is_empty() && minor.chars().all(|value| value.is_ascii_digit())
        })
}

fn is_julia(stem: &str) -> bool {
    stem == "julia"
}

const R_SCRIPT: ScriptSpec = ScriptSpec {
    label: "R",
    program: is_rscript,
    example: "Rscript analysis.R",
    extensions: &["r"],
    flags: &[
        "--vanilla",
        "--no-save",
        "--no-restore",
        "--no-environ",
        "--no-site-file",
        "--no-init-file",
        "--no-echo",
        "--quiet",
        "--verbose",
    ],
    flag_prefixes: &["--default-packages="],
};

const PYTHON_SCRIPT: ScriptSpec = ScriptSpec {
    label: "Python",
    program: is_python,
    example: "python3 analysis.py",
    extensions: &["py"],
    flags: &["-u", "-B", "-E", "-I", "-s", "-S", "-O", "-OO"],
    flag_prefixes: &["-W"],
};

const JULIA_SCRIPT: ScriptSpec = ScriptSpec {
    label: "Julia",
    program: is_julia,
    example: "julia --project=. analysis.jl",
    extensions: &["jl"],
    flags: &[
        "--project",
        "--startup-file=no",
        "--history-file=no",
        "--color=no",
        "-q",
        "--quiet",
    ],
    flag_prefixes: &["--project=", "--threads=", "--optimize="],
};

fn parse_script(argv: &[String], windows: bool, spec: &ScriptSpec) -> Result<AdapterShape, String> {
    reject_operators(argv)?;
    if !(spec.program)(&program_stem(&argv[0], windows)) {
        return Err(format!(
            "{} profiles start with the interpreter, for example {}",
            spec.label, spec.example
        ));
    }
    let mut index = 1;
    while let Some(option) = argv.get(index).filter(|value| value.starts_with('-')) {
        let allowed = spec.flags.contains(&option.as_str())
            || spec
                .flag_prefixes
                .iter()
                .any(|prefix| option.len() > prefix.len() && option.starts_with(prefix));
        if !allowed {
            return Err(format!(
                "{} option {option} is not available in this adapter. Put the code in a script inside the project, or use the command adapter.",
                spec.label
            ));
        }
        index += 1;
    }
    let script = argv.get(index).ok_or_else(|| {
        format!(
            "{} profiles need a script after the interpreter, for example {}",
            spec.label, spec.example
        )
    })?;
    project_file(spec.label, script, spec.extensions, windows)?;
    Ok(AdapterShape {
        script: Some(script.clone()),
        ..AdapterShape::default()
    })
}

/// Split an `--option=value` or `--option value` pair.
fn option_value(
    argv: &[String],
    index: &mut usize,
    inline: Option<&str>,
    name: &str,
) -> Result<String, String> {
    if let Some(value) = inline {
        return Ok(value.to_owned());
    }
    *index += 1;
    argv.get(*index)
        .filter(|value| !value.starts_with('-'))
        .cloned()
        .ok_or_else(|| format!("{name} needs a value"))
}

fn split_option(argument: &str) -> (&str, Option<&str>) {
    match argument.split_once('=') {
        Some((name, value)) if name.starts_with("--") => (name, Some(value)),
        _ => (argument, None),
    }
}

fn parent_folder(path: &str, windows: bool) -> &str {
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    path.rsplit_once(separators)
        .map(|(parent, _)| parent)
        .unwrap_or("")
}

fn parse_notebook(argv: &[String], windows: bool) -> Result<AdapterShape, String> {
    const SHAPE: &str = "Notebook profiles run jupyter nbconvert --execute --to notebook --output <executed copy> <notebook>.ipynb";
    reject_operators(argv)?;
    if program_stem(&argv[0], windows) != "jupyter"
        || argv.get(1).map(String::as_str) != Some("nbconvert")
    {
        return Err(SHAPE.into());
    }
    let (mut execute, mut to, mut output, mut output_dir, mut input) =
        (false, None, None, None, None);
    let mut index = 2;
    while index < argv.len() {
        let argument = &argv[index];
        let (name, inline) = split_option(argument);
        match name {
            "--execute" if inline.is_none() => execute = true,
            "--to" => to = Some(option_value(argv, &mut index, inline, name)?),
            "--output" => output = Some(option_value(argv, &mut index, inline, name)?),
            "--output-dir" => output_dir = Some(option_value(argv, &mut index, inline, name)?),
            "--ExecutePreprocessor.timeout" | "--ExecutePreprocessor.kernel_name"
                if inline.is_some_and(|value| !value.is_empty()) => {}
            "--inplace" => {
                return Err("Pipeline keeps the source notebook unchanged; use --output for the executed copy instead of --inplace.".into())
            }
            "--allow-errors" => {
                return Err("Remove --allow-errors so a failing cell fails the check.".into())
            }
            _ if argument.starts_with('-') => {
                return Err(format!("nbconvert option {argument} is not available in this adapter"))
            }
            _ => {
                if input.replace(argument.clone()).is_some() {
                    return Err("Execute one notebook per profile".into());
                }
            }
        }
        index += 1;
    }
    if !execute {
        return Err("Add --execute so the notebook runs before it is saved".into());
    }
    let suffix = match to.as_deref() {
        Some("notebook") => ".ipynb",
        Some("html") => ".html",
        _ => {
            return Err(
                "Add --to notebook (or --to html) to choose the executed copy's format".into(),
            )
        }
    };
    let input = input.ok_or(SHAPE)?;
    project_file("Notebook", &input, &["ipynb"], windows)?;
    let output = output.ok_or("Add --output with a file name for the executed copy, for example --output analysis.executed.ipynb")?;
    let separators: &[char] = if windows { &['/', '\\'] } else { &['/'] };
    if output.is_empty() || output.contains(separators) || output.starts_with('.') {
        return Err("--output must be a plain file name; use --output-dir for a folder".into());
    }
    let file = if output.to_ascii_lowercase().ends_with(suffix) {
        output
    } else {
        format!("{output}{suffix}")
    };
    let folder = match output_dir {
        Some(folder) => {
            safe_relative_path("Notebook output folder", &folder).map_err(|_| {
                "--output-dir must be a folder inside the working folder".to_string()
            })?;
            folder
        }
        None => parent_folder(&input, windows).to_owned(),
    };
    let required = if folder.is_empty() {
        file
    } else {
        format!("{folder}/{file}")
    };
    if same_relative(&required, &input, windows) {
        return Err(
            "The executed copy would replace the source notebook; choose a different --output name"
                .into(),
        );
    }
    Ok(AdapterShape {
        script: Some(input),
        required_output: Some(required),
        stata_log: None,
    })
}

fn parse_quarto(argv: &[String], windows: bool) -> Result<AdapterShape, String> {
    const SHAPE: &str = "Quarto profiles run quarto render <document>.qmd, optionally followed by --to, --output, --output-dir, --profile, -P or --execute";
    reject_operators(argv)?;
    if program_stem(&argv[0], windows) != "quarto"
        || argv.get(1).map(String::as_str) != Some("render")
    {
        return Err(SHAPE.into());
    }
    let input = argv
        .get(2)
        .filter(|value| !value.starts_with('-'))
        .ok_or(SHAPE)?;
    project_file("Quarto", input, &["qmd", "rmd", "ipynb", "md"], windows)?;
    let mut index = 3;
    while index < argv.len() {
        let (name, inline) = split_option(&argv[index]);
        match name {
            "--to" | "-t" | "--profile" | "-P" => {
                option_value(argv, &mut index, inline, name)?;
            }
            "--output" | "-o" => {
                let value = option_value(argv, &mut index, inline, name)?;
                if value == "-" {
                    return Err("Write the rendered document to a file, not standard output".into());
                }
            }
            "--output-dir" => {
                let value = option_value(argv, &mut index, inline, name)?;
                safe_relative_path("Quarto output folder", &value).map_err(|_| {
                    "--output-dir must be a folder inside the working folder".to_string()
                })?;
            }
            "--execute" | "--quiet" | "--no-cache" | "--cache-refresh" | "--cache"
                if inline.is_none() => {}
            _ => {
                return Err(format!(
                    "Quarto option {} is not available in this adapter",
                    argv[index]
                ))
            }
        }
        index += 1;
    }
    Ok(AdapterShape {
        script: Some(input.clone()),
        ..AdapterShape::default()
    })
}

fn parse_stata_direct(argv: &[String], windows: bool) -> Result<AdapterShape, String> {
    reject_operators(argv).map_err(|_| STATA_SHAPE.to_string())?;
    if !is_stata_program(&program_stem(&argv[0], windows)) {
        return Err(STATA_SHAPE.into());
    }
    let (batch, quiet) = if windows { ("/e", "/q") } else { ("-b", "-q") };
    let mut index = 1;
    let mut batch_seen = false;
    while let Some(argument) = argv.get(index) {
        let flag = if windows {
            argument.to_ascii_lowercase()
        } else {
            argument.clone()
        };
        if flag == batch && !batch_seen {
            batch_seen = true;
        } else if flag != quiet {
            break;
        }
        index += 1;
    }
    if !batch_seen || argv.get(index).map(String::as_str) != Some("do") {
        return Err(STATA_SHAPE.into());
    }
    let script = argv.get(index + 1).ok_or(STATA_SHAPE)?;
    project_file("Stata", script, &["do"], windows)?;
    let name = file_name(script, windows);
    let stem = &name[..name.len() - 3];
    Ok(AdapterShape {
        script: Some(script.clone()),
        required_output: None,
        stata_log: Some(format!("{stem}.log")),
    })
}

fn parse_stata_launcher(argv: &[String]) -> Result<AdapterShape, String> {
    if !matches!(argv[1].as_str(), "-lic" | "-ilc" | "-ic") {
        return Err(LAUNCHER_SHAPE.into());
    }
    let words = shell_words(&argv[2]).ok_or(LAUNCHER_SHAPE)?;
    let alias = words.first().ok_or(LAUNCHER_SHAPE)?;
    if alias.is_empty()
        || alias.len() > 64
        || !alias
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '.'))
    {
        return Err(LAUNCHER_SHAPE.into());
    }
    let mut index = 1;
    let mut batch_seen = false;
    while let Some(word) = words.get(index) {
        match word.as_str() {
            "-b" if !batch_seen => batch_seen = true,
            "-q" => {}
            _ => break,
        }
        index += 1;
    }
    if !batch_seen || words.get(index).map(String::as_str) != Some("do") {
        return Err(LAUNCHER_SHAPE.into());
    }
    let script = words.get(index + 1).ok_or(LAUNCHER_SHAPE)?;
    // Launchers predate relative-path enforcement: an absolute do-file stays
    // accepted, but a relative one must stay inside the working folder.
    if !(script.starts_with('/') || safe_relative_path("Stata do-file", script).is_ok())
        || !extension(script, false).is_some_and(|value| value == "do")
    {
        return Err(LAUNCHER_SHAPE.into());
    }
    Ok(AdapterShape::default())
}

/// Split a launcher command into words: plain characters, single-quoted text,
/// double-quoted text without expansions, and backslash-escaped characters.
/// Anything a shell could expand or chain is refused.
pub(super) fn shell_words(command: &str) -> Option<Vec<String>> {
    if command.contains(LAUNCHER_OPERATORS) || command.contains('\0') {
        return None;
    }
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut characters = command.chars();
    while let Some(value) = characters.next() {
        match value {
            ' ' | '\t' => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next()? {
                        '\'' => break,
                        quoted => current.push(quoted),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match characters.next()? {
                        '"' => break,
                        '\\' => return None,
                        quoted => current.push(quoted),
                    }
                }
            }
            '\\' => {
                started = true;
                current.push(characters.next()?);
            }
            '=' if !started => return None,
            plain if plain.is_alphanumeric() || "-_./,:+@%=".contains(plain) => {
                started = true;
                current.push(plain);
            }
            _ => return None,
        }
    }
    if started {
        words.push(current);
    }
    Some(words)
}
