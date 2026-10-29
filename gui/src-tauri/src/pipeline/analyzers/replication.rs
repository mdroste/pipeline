//! `replication_static` v1: a read-only check of a replication package
//! folder against common data-editor requirements (the AEA Data Editor's
//! preparation checklist and the Social Science Data Editors README
//! template). It reads files only; it never runs code. Each check reports
//! `pass`, `warn`, `fail`, or `info` with file and line locations.

use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const DEFAULT_MAX_FILES: usize = 5_000;
const MAX_TOTAL_SCANNED_BYTES: u64 = 200 * 1024 * 1024;
const MAX_CODE_FILE_BYTES: u64 = 2 * 1024 * 1024;
const LARGE_FILE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_LOCATIONS: usize = 25;
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    ".Rproj.user",
    "renv/library",
    ".ipynb_checkpoints",
];
const CODE_EXTENSIONS: &[&str] = &[
    "do", "ado", "r", "rmd", "qmd", "py", "ipynb", "m", "jl", "sh", "sas", "sps", "mod", "inc",
];

#[derive(Debug, Clone, Serialize)]
pub struct Location {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    /// `pass`, `warn`, `fail`, or `info`.
    pub status: &'static str,
    pub detail: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<Location>,
}

struct Inventory {
    root: PathBuf,
    /// Root-relative, forward-slash paths with sizes.
    files: Vec<(String, u64)>,
    truncated: bool,
}

fn inventory(root: &Path, max_files: usize) -> Inventory {
    let mut files = Vec::new();
    let mut truncated = false;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if truncated {
            break;
        }
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default();
            if kind.is_dir() {
                let skip = SKIPPED_DIRS.iter().any(|skipped| {
                    relative == *skipped || relative.ends_with(&format!("/{skipped}"))
                });
                if !skip {
                    stack.push(path);
                }
            } else if kind.is_file() {
                if files.len() >= max_files {
                    truncated = true;
                    break;
                }
                let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
                files.push((relative, size));
            }
        }
    }
    files.sort();
    Inventory {
        root: root.to_path_buf(),
        files,
        truncated,
    }
}

fn extension(path: &str) -> String {
    path.rsplit('.')
        .next()
        .filter(|ext| ext.len() < path.len())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase()
}

/// Code files read for line checks, within per-file and total byte limits.
fn code_files(inventory: &Inventory) -> Vec<(String, String)> {
    let mut total = 0u64;
    let mut files = Vec::new();
    for (path, size) in &inventory.files {
        if !CODE_EXTENSIONS.contains(&extension(path).as_str()) || *size > MAX_CODE_FILE_BYTES {
            continue;
        }
        if total + size > MAX_TOTAL_SCANNED_BYTES {
            break;
        }
        let Ok(bytes) = std::fs::read(inventory.root.join(path)) else {
            continue;
        };
        total += size;
        files.push((path.clone(), String::from_utf8_lossy(&bytes).into_owned()));
    }
    files
}

fn snippet(line: &str) -> String {
    line.trim().chars().take(160).collect()
}

fn scan(
    files: &[(String, String)],
    filter: impl Fn(&str) -> bool,
    pattern: &Regex,
) -> Vec<Location> {
    let mut found = Vec::new();
    for (path, text) in files.iter().filter(|(path, _)| filter(path)) {
        for (index, line) in text.lines().enumerate() {
            let code = strip_comment(path, line);
            if pattern.is_match(code) {
                found.push(Location {
                    path: path.clone(),
                    line: Some(index + 1),
                    snippet: snippet(line),
                });
                if found.len() >= MAX_LOCATIONS {
                    return found;
                }
            }
        }
    }
    found
}

/// The part of a line before a line comment, by language.
fn strip_comment<'a>(path: &str, line: &'a str) -> &'a str {
    let trimmed = line.trim_start();
    let ext = extension(path);
    let starts_comment = match ext.as_str() {
        "do" | "ado" => trimmed.starts_with('*') || trimmed.starts_with("//"),
        "m" => trimmed.starts_with('%'),
        "sas" | "sps" => trimmed.starts_with('*'),
        _ => trimmed.starts_with('#'),
    };
    if starts_comment {
        return "";
    }
    let marker = match ext.as_str() {
        "do" | "ado" => " //",
        "m" => " %",
        "r" | "py" | "jl" | "sh" | "rmd" | "qmd" => " #",
        _ => return line,
    };
    line.find(marker).map(|at| &line[..at]).unwrap_or(line)
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("valid pattern"))
}

struct ReadmeSection {
    label: &'static str,
    keywords: &'static [&'static str],
}

const README_SECTIONS: &[ReadmeSection] = &[
    ReadmeSection {
        label: "Data availability and provenance",
        keywords: &[
            "data availability",
            "provenance",
            "data sources",
            "availability of data",
        ],
    },
    ReadmeSection {
        label: "Rights and licenses for data",
        keywords: &["rights", "license", "licence", "permission", "terms of use"],
    },
    ReadmeSection {
        label: "Computational requirements (software and versions)",
        keywords: &[
            "computational requirements",
            "software requirements",
            "software",
            "stata version",
            "r version",
            "python version",
        ],
    },
    ReadmeSection {
        label: "Memory, runtime, and storage",
        keywords: &[
            "runtime",
            "run time",
            "running time",
            "hours",
            "memory",
            "storage",
            "ram",
            "approximate time",
        ],
    },
    ReadmeSection {
        label: "Controlled randomness (seeds)",
        keywords: &["seed", "random"],
    },
    ReadmeSection {
        label: "Description of programs",
        keywords: &[
            "description of programs",
            "programs",
            "code description",
            "description of code",
            "scripts",
        ],
    },
    ReadmeSection {
        label: "Instructions to replicators",
        keywords: &[
            "instructions",
            "to replicate",
            "replicators",
            "how to run",
            "run the",
        ],
    },
    ReadmeSection {
        label: "List of tables, figures, and programs",
        keywords: &[
            "list of tables",
            "tables and figures",
            "table 1",
            "figure 1",
            "exhibits",
        ],
    },
    ReadmeSection {
        label: "Data citations / references",
        keywords: &["references", "citation", "cite"],
    },
];

fn readme_checks(inventory: &Inventory, checks: &mut Vec<Check>) {
    let readmes: Vec<&String> = inventory
        .files
        .iter()
        .map(|(path, _)| path)
        .filter(|path| !path.contains('/') && file_name(path).starts_with("readme"))
        .collect();
    let Some(readme) = readmes
        .iter()
        .find(|path| {
            matches!(
                extension(path).as_str(),
                "md" | "txt" | "markdown" | "tex" | ""
            )
        })
        .or_else(|| readmes.first())
    else {
        checks.push(Check {
            id: "readme_present",
            label: "README at the top level",
            status: "fail",
            detail: "No README was found at the top of the package. Data editors require one, following the Social Science Data Editors template.".into(),
            locations: Vec::new(),
        });
        return;
    };
    let ext = extension(readme);
    checks.push(Check {
        id: "readme_present",
        label: "README at the top level",
        status: "pass",
        detail: format!("Found {readme}. The AEA asks for a PDF README in the deposit; keep an editable copy as well."),
        locations: Vec::new(),
    });
    if !matches!(ext.as_str(), "md" | "txt" | "markdown" | "tex" | "") {
        checks.push(Check {
            id: "readme_sections",
            label: "README sections",
            status: "info",
            detail: format!("The README is a .{ext} file, which this check does not read. Compare it with the template sections manually."),
            locations: Vec::new(),
        });
        return;
    }
    let text = std::fs::read(inventory.root.join(readme))
        .map(|bytes| String::from_utf8_lossy(&bytes).to_lowercase())
        .unwrap_or_default();
    let missing: Vec<&str> = README_SECTIONS
        .iter()
        .filter(|section| {
            !section
                .keywords
                .iter()
                .any(|keyword| text.contains(keyword))
        })
        .map(|section| section.label)
        .collect();
    checks.push(Check {
        id: "readme_sections",
        label: "README sections",
        status: if missing.is_empty() { "pass" } else if missing.len() <= 2 { "warn" } else { "fail" },
        detail: if missing.is_empty() {
            "The README mentions every section of the template.".into()
        } else {
            format!("The README does not appear to cover: {}. Wording varies, so confirm by reading it.", missing.join("; "))
        },
        locations: Vec::new(),
    });
}

const MAIN_SCRIPTS: &[&str] = &[
    "main.do",
    "master.do",
    "run_all.do",
    "runall.do",
    "0_master.do",
    "00_master.do",
    "_master.do",
    "main.r",
    "master.r",
    "run_all.r",
    "run.r",
    "main.py",
    "run_all.py",
    "master.py",
    "makefile",
    "main.m",
    "master.m",
    "run_all.m",
    "main.jl",
    "run.sh",
    "run_all.sh",
    "main.sh",
];

fn main_script_check(inventory: &Inventory, code_count: usize, checks: &mut Vec<Check>) {
    let found: Vec<Location> = inventory
        .files
        .iter()
        .filter(|(path, _)| {
            path.matches('/').count() <= 1 && MAIN_SCRIPTS.contains(&file_name(path).as_str())
        })
        .map(|(path, _)| Location {
            path: path.clone(),
            line: None,
            snippet: String::new(),
        })
        .collect();
    checks.push(Check {
        id: "main_script",
        label: "One main script runs everything",
        status: if !found.is_empty() { "pass" } else if code_count == 0 { "info" } else { "fail" },
        detail: if !found.is_empty() {
            "A main script was found. Confirm that it runs every program in order.".into()
        } else if code_count == 0 {
            "No code files were found.".into()
        } else {
            "No main script (for example main.do, master.do, main.R, or a Makefile) was found at the top level. Data editors ask for one script, or one per language, that runs the whole package.".into()
        },
        locations: found,
    });
}

fn path_checks(files: &[(String, String)], checks: &mut Vec<Check>) {
    static ABSOLUTE: OnceLock<Regex> = OnceLock::new();
    static BACKSLASH: OnceLock<Regex> = OnceLock::new();
    let absolute = regex(
        &ABSOLUTE,
        r#"(?i)["'`(\s=]((?:[a-z]:[\\/])|(?:/users/)|(?:/home/)|(?:~/)|(?:\\\\[a-z0-9_.-]+\\))"#,
    );
    let found = scan(files, |_| true, absolute);
    checks.push(Check {
        id: "absolute_paths",
        label: "No hard-coded absolute paths",
        status: if found.is_empty() { "pass" } else { "fail" },
        detail: if found.is_empty() {
            "No absolute paths were found in code.".into()
        } else {
            "Code refers to paths on a specific computer. Set one project root dynamically and make every path relative to it.".into()
        },
        locations: found,
    });
    let backslash = regex(
        &BACKSLASH,
        r#""[^"\n]*[A-Za-z0-9_.)]\\[A-Za-z0-9_.$]+[^"\n]*""#,
    );
    let found = scan(
        files,
        |path| matches!(extension(path).as_str(), "do" | "ado"),
        backslash,
    );
    checks.push(Check {
        id: "backslash_paths",
        label: "Forward slashes in Stata paths",
        status: if found.is_empty() { "pass" } else { "warn" },
        detail: if found.is_empty() {
            "No backslash paths were found in Stata code.".into()
        } else {
            "Stata paths with backslashes fail on macOS and Linux. Use forward slashes, which work on every platform.".into()
        },
        locations: found,
    });
}

fn package_checks(inventory: &Inventory, files: &[(String, String)], checks: &mut Vec<Check>) {
    static STATA_INSTALL: OnceLock<Regex> = OnceLock::new();
    static R_INSTALL: OnceLock<Regex> = OnceLock::new();
    static PIP_INSTALL: OnceLock<Regex> = OnceLock::new();
    let has = |names: &[&str]| {
        inventory
            .files
            .iter()
            .any(|(path, _)| names.contains(&file_name(path).as_str()))
    };
    let languages = |exts: &[&str]| {
        files
            .iter()
            .any(|(path, _)| exts.contains(&extension(path).as_str()))
    };
    let stata = scan(
        files,
        |path| matches!(extension(path).as_str(), "do" | "ado"),
        regex(&STATA_INSTALL, r"(?i)^\s*(ssc|net)\s+install\b"),
    );
    if languages(&["do", "ado"]) {
        let project_ado = inventory
            .files
            .iter()
            .any(|(path, _)| path.starts_with("ado/") || path.contains("/ado/"));
        let installs_by_code = !stata.is_empty();
        checks.push(Check {
            id: "stata_packages",
            label: "Stata packages installed by code into the project",
            status: if project_ado || installs_by_code { "pass" } else { "warn" },
            detail: if project_ado {
                "A project ado folder was found.".into()
            } else if installs_by_code {
                "Packages are installed by code. The AEA Data Editor recommends installing them into a project ado folder so the package does not depend on the replicator's own setup.".into()
            } else {
                "No package installation code or project ado folder was found. List every community package and install it by code.".into()
            },
            locations: stata,
        });
    }
    if languages(&["r", "rmd", "qmd"]) {
        let installs = scan(
            files,
            |path| matches!(extension(path).as_str(), "r" | "rmd" | "qmd"),
            regex(&R_INSTALL, r"install\.packages\s*\("),
        );
        let renv = has(&["renv.lock"]);
        checks.push(Check {
            id: "r_packages",
            label: "R package versions recorded",
            status: if renv { "pass" } else { "warn" },
            detail: if renv {
                "renv.lock records package versions.".into()
            } else {
                "No renv.lock was found. Record package versions with renv, or explain the alternative in the README.".into()
            },
            locations: installs,
        });
    }
    if languages(&["py", "ipynb"]) {
        let declared = has(&[
            "requirements.txt",
            "environment.yml",
            "environment.yaml",
            "pyproject.toml",
            "uv.lock",
            "pipfile.lock",
            "poetry.lock",
        ]);
        checks.push(Check {
            id: "python_packages",
            label: "Python environment declared",
            status: if declared { "pass" } else { "warn" },
            detail: if declared {
                "A Python environment or requirements file was found.".into()
            } else {
                "No requirements.txt, environment.yml, or lockfile was found. List the top-level packages and their versions.".into()
            },
            locations: scan(files, |path| extension(path) == "py", regex(&PIP_INSTALL, r"pip\s+install")),
        });
    }
}

fn seed_checks(files: &[(String, String)], checks: &mut Vec<Check>) {
    let rules: &[(&[&str], &str, &str)] = &[
        (
            &["do", "ado"],
            r"(?i)\b(runiform|rnormal|rbinomial|rpoisson|bootstrap|bsample|simulate|permute|sample)\b",
            r"(?i)set\s+seed",
        ),
        (
            &["r", "rmd", "qmd"],
            r"\b(sample|rnorm|runif|rbinom|rpois|boot)\s*\(",
            r"set\.seed\s*\(",
        ),
        (
            &["py", "ipynb"],
            r"\b(np\.random\.|random\.(random|randint|choice|shuffle|sample)|default_rng)",
            r"(seed\s*\(|default_rng\s*\(\s*\d|random_state\s*=)",
        ),
        (&["jl"], r"\b(rand|randn|shuffle)\s*\(", r"Random\.seed!"),
    ];
    let mut locations = Vec::new();
    for (exts, uses, seeds) in rules {
        let uses = Regex::new(uses).expect("valid");
        let seeds = Regex::new(seeds).expect("valid");
        let any_seed = files
            .iter()
            .filter(|(path, _)| exts.contains(&extension(path).as_str()))
            .any(|(_, text)| seeds.is_match(text));
        if any_seed {
            continue;
        }
        locations.extend(scan(
            files,
            |path| exts.contains(&extension(path).as_str()),
            &uses,
        ));
    }
    locations.truncate(MAX_LOCATIONS);
    checks.push(Check {
        id: "random_seeds",
        label: "Random numbers use a set seed",
        status: if locations.is_empty() { "pass" } else { "warn" },
        detail: if locations.is_empty() {
            "No random-number use without a seed was found.".into()
        } else {
            "Code draws random numbers but no seed is set in that language's files, so results may differ between runs.".into()
        },
        locations,
    });
}

fn referenced_file_checks(
    inventory: &Inventory,
    files: &[(String, String)],
    checks: &mut Vec<Check>,
) {
    static READS: OnceLock<Regex> = OnceLock::new();
    let reads = regex(
        &READS,
        r#"(?i)(?:\buse\s+|\busing\s+|\bdo\s+|\binclude\s+|\binsheet\s+using\s+|import\s+delimited\s+(?:using\s+)?|import\s+excel\s+(?:using\s+)?|read[._](?:csv|dta|table|delim|excel|rds|parquet)\w*\s*\(\s*|read_(?:csv|stata|excel|parquet)\s*\(\s*|source\s*\(\s*|load\s*\(\s*|open\s*\(\s*)["']([^"'$`{}]+\.[A-Za-z0-9]{1,6})["']"#,
    );
    let known: std::collections::HashSet<String> = inventory
        .files
        .iter()
        .map(|(path, _)| path.to_ascii_lowercase())
        .collect();
    let mut missing = Vec::new();
    for (path, text) in files {
        let dir = path.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        for (index, line) in text.lines().enumerate() {
            let code = strip_comment(path, line);
            // Lines that write outputs name files that need not exist yet.
            if exports().is_match(code) || code.trim_start().starts_with("save") {
                continue;
            }
            for captures in reads.captures_iter(code) {
                let target = captures[1].replace('\\', "/");
                if target.starts_with('/') || target.contains(':') || target.starts_with("~") {
                    continue;
                }
                let target = target.trim_start_matches("./").to_ascii_lowercase();
                let beside = if dir.is_empty() {
                    target.clone()
                } else {
                    format!("{}/{target}", dir.to_ascii_lowercase())
                };
                let exists = known.contains(&target)
                    || known.contains(&beside)
                    || known
                        .iter()
                        .any(|known| known.ends_with(&format!("/{target}")));
                if !exists {
                    missing.push(Location {
                        path: path.clone(),
                        line: Some(index + 1),
                        snippet: snippet(line),
                    });
                }
            }
            if missing.len() >= MAX_LOCATIONS {
                break;
            }
        }
    }
    checks.push(Check {
        id: "referenced_files",
        label: "Files read by code are in the package",
        status: if missing.is_empty() { "pass" } else { "warn" },
        detail: if missing.is_empty() {
            "Every literal file path read by code was found. Paths built from macros or variables are not checked.".into()
        } else {
            "Code reads files that are not in the package. If they are restricted or too large to deposit, the README must say how to obtain them.".into()
        },
        locations: missing,
    });
}

/// Commands that write tables, figures, or data files.
fn exports() -> &'static Regex {
    static EXPORTS: OnceLock<Regex> = OnceLock::new();
    regex(
        &EXPORTS,
        r"(?i)(esttab\s+using|estout\s+using|outreg2?\b.*using|graph\s+export|putexcel|texsave|outsheet|export\s+(delimited|excel)|ggsave\s*\(|write\.(csv|table)|write_csv|stargazer\s*\(.*out\s*=|modelsummary\s*\(.*output\s*=|etable\s*\(.*file|savefig\s*\(|to_latex\s*\(|to_csv\s*\(|print\s*\(.*-dpng|saveas\s*\()",
    )
}

fn output_checks(files: &[(String, String)], checks: &mut Vec<Check>) {
    let found = scan(files, |_| true, exports());
    checks.push(Check {
        id: "exported_outputs",
        label: "Tables and figures are written to files",
        status: if found.is_empty() { if files.is_empty() { "info" } else { "warn" } } else { "pass" },
        detail: if found.is_empty() {
            "No commands that write tables or figures to files were found. Every table and figure in the paper should be produced as a named file.".into()
        } else {
            format!("{} export commands were found. Map each output file to its table or figure in the README.", found.len())
        },
        locations: found.into_iter().take(10).collect(),
    });
}

fn size_checks(inventory: &Inventory, checks: &mut Vec<Check>) {
    let large: Vec<Location> = inventory
        .files
        .iter()
        .filter(|(_, size)| *size >= LARGE_FILE_BYTES)
        .map(|(path, size)| Location {
            path: path.clone(),
            line: None,
            snippet: format!("{:.1} GB", *size as f64 / 1e9),
        })
        .take(MAX_LOCATIONS)
        .collect();
    checks.push(Check {
        id: "large_files",
        label: "Very large files",
        status: if large.is_empty() { "pass" } else { "info" },
        detail: if large.is_empty() {
            "No file is 2 GB or larger.".into()
        } else {
            "Some files are 2 GB or larger. Check the repository's size limits and whether the data may be deposited.".into()
        },
        locations: large,
    });
    let license = inventory.files.iter().any(|(path, _)| {
        !path.contains('/')
            && (file_name(path).starts_with("license") || file_name(path).starts_with("licence"))
    });
    checks.push(Check {
        id: "license",
        label: "License file",
        status: if license { "pass" } else { "warn" },
        detail: if license {
            "A license file was found at the top level.".into()
        } else {
            "No LICENSE file was found. State the license for code and, where permitted, for data."
                .into()
        },
        locations: Vec::new(),
    });
}

fn findings(checks: &[Check]) -> Vec<Value> {
    checks
        .iter()
        .filter(|check| matches!(check.status, "fail" | "warn"))
        .map(|check| {
            let evidence: Vec<Value> = check
                .locations
                .iter()
                .take(10)
                .map(|location| {
                    let mut item = json!({
                        "evidence_type": "source",
                        "verification_status": "unverified",
                        "source_path": location.path,
                        "description": location.path,
                    });
                    if let Some(line) = location.line {
                        item["line_start"] = json!(line);
                        item["quote"] = json!(location.snippet);
                    }
                    item
                })
                .collect();
            json!({
                "id": format!("replication-{}", check.id.replace('_', "-")),
                "title": check.label,
                "category": "Replication package",
                "severity": if check.status == "fail" { "high" } else { "medium" },
                "confidence": "medium",
                "verification_status": "unverified",
                "problem": check.detail,
                "consequence": "A data editor or replicator may be unable to run the package or match its outputs to the paper.",
                "recommended_action": check.detail,
                "body": check.detail,
                "evidence": evidence,
            })
        })
        .collect()
}

pub fn run(root: &Path, options: &Value) -> Result<Value, String> {
    if !root.is_dir() {
        return Err("The replication package check needs a folder as its input".to_string());
    }
    let max_files = options
        .get("max_files")
        .and_then(Value::as_u64)
        .map(|count| count as usize)
        .unwrap_or(DEFAULT_MAX_FILES);
    let inventory = inventory(root, max_files);
    let code = code_files(&inventory);
    let mut checks = Vec::new();
    readme_checks(&inventory, &mut checks);
    main_script_check(&inventory, code.len(), &mut checks);
    path_checks(&code, &mut checks);
    package_checks(&inventory, &code, &mut checks);
    seed_checks(&code, &mut checks);
    referenced_file_checks(&inventory, &code, &mut checks);
    output_checks(&code, &mut checks);
    size_checks(&inventory, &mut checks);
    let mut summary = std::collections::BTreeMap::<&str, usize>::new();
    for check in &checks {
        *summary.entry(check.status).or_default() += 1;
    }
    Ok(json!({
        "analyzer": super::registry::REPLICATION_STATIC,
        "version": 1,
        "files": inventory.files.len(),
        "code_files": code.len(),
        "inventory_truncated": inventory.truncated,
        "summary": summary,
        "meaning": "Static checks of files only; nothing was run. Passing them does not show that the package reproduces the paper.",
        "standards": ["AEA Data Editor: preparing a replication package (2025-12-04)", "Social Science Data Editors template README v1.1"],
        "checks": checks,
        "findings": findings(&checks),
    }))
}

#[cfg(test)]
#[path = "replication_tests.rs"]
mod tests;
