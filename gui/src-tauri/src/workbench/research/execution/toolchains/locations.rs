//! Standard install locations and PATH lookups for each platform.

use super::probe::{folder_version, matlab_release, plist_version};
use super::Candidate;
use crate::workbench::research::execution::adapters::{is_stata_program, program_stem};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::workbench::research::execution) enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub(in crate::workbench::research::execution) fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    pub(in crate::workbench::research::execution) fn name(self) -> &'static str {
        match self {
            Self::MacOs => "macos",
            Self::Windows => "windows",
            Self::Linux => "linux",
        }
    }

    pub(in crate::workbench::research::execution) fn windows(self) -> bool {
        self == Self::Windows
    }
}

/// The folders and PATH detection reads. Tests substitute temporary folders.
#[derive(Debug, Clone)]
pub(in crate::workbench::research::execution) struct Host {
    pub platform: Platform,
    pub path: OsString,
    pub pathext: Option<OsString>,
    pub home: Option<PathBuf>,
    /// macOS application folders (`/Applications`, `~/Applications`).
    pub applications: Vec<PathBuf>,
    /// macOS framework folders (`/Library/Frameworks`).
    pub frameworks: Vec<PathBuf>,
    /// Windows Program Files folders.
    pub program_files: Vec<PathBuf>,
    /// Windows per-user program folders (`%LOCALAPPDATA%\Programs`).
    pub user_programs: Vec<PathBuf>,
    /// Linux install prefixes (`/usr/local`, `/opt`).
    pub prefixes: Vec<PathBuf>,
    pub probe_versions: bool,
}

impl Host {
    pub(in crate::workbench::research::execution) fn current() -> Self {
        let platform = Platform::current();
        let home = dirs::home_dir();
        let mut host = Self {
            platform,
            path: OsString::from(crate::env::full_path()),
            pathext: std::env::var_os("PATHEXT"),
            home: home.clone(),
            applications: Vec::new(),
            frameworks: Vec::new(),
            program_files: Vec::new(),
            user_programs: Vec::new(),
            prefixes: Vec::new(),
            probe_versions: true,
        };
        match platform {
            Platform::MacOs => {
                host.applications.push(PathBuf::from("/Applications"));
                host.applications
                    .extend(home.map(|home| home.join("Applications")));
                host.frameworks.push(PathBuf::from("/Library/Frameworks"));
            }
            Platform::Windows => {
                for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
                    if let Some(folder) = std::env::var_os(variable).map(PathBuf::from) {
                        if !host.program_files.contains(&folder) {
                            host.program_files.push(folder);
                        }
                    }
                }
                if host.program_files.is_empty() {
                    host.program_files.push(PathBuf::from(r"C:\Program Files"));
                }
                host.user_programs.extend(
                    std::env::var_os("LOCALAPPDATA")
                        .map(|local| PathBuf::from(local).join("Programs")),
                );
            }
            Platform::Linux => {
                host.prefixes.push(PathBuf::from("/usr/local"));
                host.prefixes.push(PathBuf::from("/opt"));
            }
        }
        host
    }
}

const SCAN_LIMIT: usize = 512;
const UNIX_STATA: [&str; 5] = ["stata-mp", "stata-se", "stata-be", "stata-ic", "stata"];

/// Sorted child paths whose file name satisfies `keep`; bounded per folder.
fn children(folder: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found = entries
        .take(SCAN_LIMIT)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(&keep))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    found.sort();
    found
}

fn nested(base: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

fn name_of(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

fn lower_starts(name: &str, prefix: &str) -> bool {
    name.to_ascii_lowercase().starts_with(prefix)
}

pub(super) fn launchable(path: &Path, platform: Platform) -> bool {
    if platform.windows() {
        return path.is_file();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

struct Found<'a> {
    host: &'a Host,
    items: Vec<Candidate>,
}

impl Found<'_> {
    fn push(
        &mut self,
        kind: &'static str,
        path: PathBuf,
        source: &'static str,
        label: String,
        version_hint: Option<(String, &'static str)>,
    ) {
        if !launchable(&path, self.host.platform) {
            return;
        }
        let jupyter = (kind == "python").then(|| {
            let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
            if self.host.platform.windows() {
                folder.join("Scripts").join("jupyter.exe")
            } else {
                folder.join("jupyter")
            }
        });
        self.items.push(Candidate {
            kind,
            path,
            source,
            label: label.clone(),
            version_hint,
        });
        if let Some(jupyter) = jupyter {
            self.push(
                "jupyter",
                jupyter,
                source,
                format!("Jupyter with {label}"),
                None,
            );
        }
    }
}

fn stata_edition(executable: &str, windows: bool) -> &'static str {
    let stem = program_stem(executable, windows);
    let stem = stem.strip_suffix("-64").unwrap_or(&stem).to_owned();
    if stem.ends_with("mp") {
        "MP"
    } else if stem.ends_with("se") {
        "SE"
    } else if stem.ends_with("be") {
        "BE"
    } else if stem.ends_with("ic") {
        "IC"
    } else {
        ""
    }
}

fn stata_label(folder: &Path, executable: &Path, windows: bool) -> String {
    format!(
        "{} {}",
        name_of(folder),
        stata_edition(name_of(executable), windows)
    )
    .trim()
    .to_owned()
}

fn hint(version: Option<String>, source: &'static str) -> Option<(String, &'static str)> {
    version.map(|version| (version, source))
}

/// Executables in standard locations, then PATH entries.
pub(in crate::workbench::research::execution) fn candidates(host: &Host) -> Vec<Candidate> {
    let mut found = Found {
        host,
        items: Vec::new(),
    };
    match host.platform {
        Platform::MacOs => mac(&mut found),
        Platform::Windows => windows(&mut found),
        Platform::Linux => linux(&mut found),
    }
    if !host.platform.windows() {
        if let Some(home) = &host.home {
            let juliaup = home.join(".juliaup").join("bin").join("julia");
            found.push(
                "julia",
                juliaup,
                "standardLocation",
                "Julia (juliaup)".into(),
                None,
            );
        }
    }
    on_path(&mut found);
    found.items
}

fn mac(found: &mut Found<'_>) {
    let host = found.host;
    for applications in &host.applications {
        for folder in children(applications, |name| lower_starts(name, "stata")) {
            let apps = children(&folder, |name| {
                lower_starts(name, "stata") && name.to_ascii_lowercase().ends_with(".app")
            });
            for app in apps {
                let version = plist_version(&app.join("Contents").join("Info.plist"))
                    .map(|version| (version, "install folder (Info.plist)"))
                    .or_else(|| hint(folder_version(name_of(&folder)), "install folder name"));
                // The lowercase console binaries run batch jobs; the capitalized
                // GUI binary beside them is not offered.
                let consoles = children(&app.join("Contents").join("MacOS"), |name| {
                    UNIX_STATA.contains(&name)
                });
                for executable in consoles {
                    let label = stata_label(&folder, &executable, false);
                    found.push(
                        "stata",
                        executable,
                        "standardLocation",
                        label,
                        version.clone(),
                    );
                }
            }
        }
        for app in children(applications, |name| {
            lower_starts(name, "julia-") && name.ends_with(".app")
        }) {
            let label = name_of(&app).trim_end_matches(".app").to_owned();
            let path = nested(&app, &["Contents", "Resources", "julia", "bin", "julia"]);
            found.push(
                "julia",
                path,
                "standardLocation",
                label.clone(),
                hint(folder_version(&label), "install folder name"),
            );
        }
        found.push(
            "quarto",
            nested(applications, &["quarto", "bin", "quarto"]),
            "standardLocation",
            "Quarto".into(),
            None,
        );
        found.push(
            "quarto",
            nested(
                applications,
                &[
                    "RStudio.app",
                    "Contents",
                    "Resources",
                    "app",
                    "quarto",
                    "bin",
                    "quarto",
                ],
            ),
            "standardLocation",
            "Quarto bundled with RStudio".into(),
            None,
        );
        for app in children(applications, |name| {
            name.starts_with("MATLAB_R") && name.ends_with(".app")
        }) {
            let release = matlab_release(name_of(&app));
            let label = format!("MATLAB {}", release.clone().unwrap_or_default());
            found.push(
                "matlab",
                nested(&app, &["bin", "matlab"]),
                "standardLocation",
                label.trim().into(),
                hint(release, "install folder name"),
            );
        }
    }
    for frameworks in &host.frameworks {
        found.push(
            "r",
            nested(frameworks, &["R.framework", "Resources", "bin", "Rscript"]),
            "standardLocation",
            "R (CRAN installer)".into(),
            None,
        );
        let versions = nested(frameworks, &["Python.framework", "Versions"]);
        for version in children(&versions, |name| {
            name.chars()
                .next()
                .is_some_and(|first| first.is_ascii_digit())
        }) {
            let number = name_of(&version).to_owned();
            found.push(
                "python",
                nested(&version, &["bin", "python3"]),
                "standardLocation",
                format!("Python {number} (python.org installer)"),
                Some((number, "install folder name")),
            );
        }
    }
}

fn windows(found: &mut Found<'_>) {
    let host = found.host;
    for program_files in &host.program_files {
        for folder in children(program_files, |name| lower_starts(name, "stata")) {
            let version = hint(folder_version(name_of(&folder)), "install folder name");
            for executable in children(&folder, |name| {
                name.to_ascii_lowercase().ends_with(".exe")
                    && is_stata_program(&program_stem(name, true))
            }) {
                let label = stata_label(&folder, &executable, true);
                found.push(
                    "stata",
                    executable,
                    "standardLocation",
                    label,
                    version.clone(),
                );
            }
        }
        r_versions(found, &program_files.join("R"));
        for folder in children(program_files, |name| lower_starts(name, "python3")) {
            let label = name_of(&folder).to_owned();
            found.push(
                "python",
                folder.join("python.exe"),
                "standardLocation",
                label,
                None,
            );
        }
        for folder in children(program_files, |name| lower_starts(name, "julia")) {
            let label = name_of(&folder).to_owned();
            let version = hint(folder_version(&label), "install folder name");
            found.push(
                "julia",
                nested(&folder, &["bin", "julia.exe"]),
                "standardLocation",
                label,
                version,
            );
        }
        found.push(
            "quarto",
            nested(program_files, &["Quarto", "bin", "quarto.exe"]),
            "standardLocation",
            "Quarto".into(),
            None,
        );
        for release in children(&program_files.join("MATLAB"), |name| {
            name.starts_with("R20")
        }) {
            let version = matlab_release(name_of(&release));
            let label = format!("MATLAB {}", name_of(&release));
            found.push(
                "matlab",
                nested(&release, &["bin", "matlab.exe"]),
                "standardLocation",
                label,
                hint(version, "install folder name"),
            );
        }
    }
    for programs in &host.user_programs {
        for folder in children(&programs.join("Python"), |name| {
            lower_starts(name, "python3")
        }) {
            let label = name_of(&folder).to_owned();
            found.push(
                "python",
                folder.join("python.exe"),
                "standardLocation",
                label,
                None,
            );
        }
        for folder in children(programs, |name| lower_starts(name, "julia")) {
            let label = name_of(&folder).to_owned();
            let version = hint(folder_version(&label), "install folder name");
            found.push(
                "julia",
                nested(&folder, &["bin", "julia.exe"]),
                "standardLocation",
                label,
                version,
            );
        }
        found.push(
            "quarto",
            nested(programs, &["Quarto", "bin", "quarto.exe"]),
            "standardLocation",
            "Quarto".into(),
            None,
        );
        r_versions(found, &programs.join("R"));
    }
}

fn r_versions(found: &mut Found<'_>, folder: &Path) {
    for install in children(folder, |name| lower_starts(name, "r-")) {
        let label = name_of(&install).to_owned();
        let version = hint(folder_version(&label), "install folder name");
        for binary in [&["bin", "Rscript.exe"][..], &["bin", "x64", "Rscript.exe"]] {
            found.push(
                "r",
                nested(&install, binary),
                "standardLocation",
                label.clone(),
                version.clone(),
            );
        }
    }
}

fn linux(found: &mut Found<'_>) {
    let host = found.host;
    for prefix in &host.prefixes {
        for folder in children(prefix, |name| lower_starts(name, "stata")) {
            let version = hint(folder_version(name_of(&folder)), "install folder name");
            for name in UNIX_STATA {
                let executable = folder.join(name);
                let label = stata_label(&folder, &executable, false);
                found.push(
                    "stata",
                    executable,
                    "standardLocation",
                    label,
                    version.clone(),
                );
            }
        }
        for folder in children(prefix, |name| lower_starts(name, "julia-")) {
            let label = name_of(&folder).to_owned();
            let version = hint(folder_version(&label), "install folder name");
            found.push(
                "julia",
                nested(&folder, &["bin", "julia"]),
                "standardLocation",
                label,
                version,
            );
        }
        found.push(
            "quarto",
            nested(prefix, &["quarto", "bin", "quarto"]),
            "standardLocation",
            "Quarto".into(),
            None,
        );
        for release in children(&prefix.join("MATLAB"), |name| name.starts_with("R20")) {
            let version = matlab_release(name_of(&release));
            let label = format!("MATLAB {}", name_of(&release));
            found.push(
                "matlab",
                nested(&release, &["bin", "matlab"]),
                "standardLocation",
                label,
                hint(version, "install folder name"),
            );
        }
        for version in children(&prefix.join("R"), |name| {
            name.chars()
                .next()
                .is_some_and(|first| first.is_ascii_digit())
        }) {
            let number = name_of(&version).to_owned();
            found.push(
                "r",
                nested(&version, &["bin", "Rscript"]),
                "standardLocation",
                format!("R {number}"),
                Some((number, "install folder name")),
            );
        }
    }
}

const PATH_NAMES: [(&str, &[&str]); 7] = [
    ("stata", &UNIX_STATA),
    ("r", &["Rscript"]),
    ("python", &["python3", "python"]),
    ("julia", &["julia"]),
    ("quarto", &["quarto"]),
    ("jupyter", &["jupyter"]),
    ("matlab", &["matlab"]),
];

fn on_path(found: &mut Found<'_>) {
    let host = found.host;
    let windows = host.platform.windows();
    let cwd = std::env::temp_dir();
    for (kind, names) in PATH_NAMES {
        // Stata for Windows does not put its executables on PATH.
        if windows && kind == "stata" {
            continue;
        }
        for name in names {
            let Some(resolved) = crate::deps::resolve_command_at(
                name,
                &host.path,
                &cwd,
                windows,
                host.pathext.as_deref(),
            ) else {
                continue;
            };
            found.push(
                kind,
                resolved.discovered_path().to_path_buf(),
                "path",
                format!("{name} on PATH"),
                None,
            );
        }
    }
}
