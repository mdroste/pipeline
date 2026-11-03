//! Shared resource and filesystem safety primitives.
//!
//! Keep limits here so imported profiles, generated artifacts, and local file
//! handoffs are governed by one consistent policy across the GUI and CLI.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

static SPAN_TAG_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r#"(?is)<\s*/?\s*span\b(?:[^>"']|"[^"]*"|'[^']*')*>"#)
        .expect("span-tag regex must compile")
});

pub const MAX_EXPANDED_PROMPT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RUNTIME_VALUE_BYTES: usize = 1024 * 1024;
pub const MAX_RUNTIME_CONTEXT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RUN_STEP_UNITS: u64 = 2_000;
pub const MAX_RUN_PROVIDER_ATTEMPTS: u64 = 5_000;
pub const MAX_RUN_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
pub const MAX_RUNTIME_VALUES: usize = 256;

pub const MAX_WALK_ENTRIES: usize = 100_000;
pub const MAX_WALK_DIRS: usize = 10_000;
pub const MAX_WALK_TIME: Duration = Duration::from_secs(5);

/// Remove HTML span wrappers from model-facing text without altering their
/// contents. Span IDs/classes are presentational extraction noise and provide
/// no useful semantic context to a model.
pub fn strip_span_tags(input: &str) -> std::borrow::Cow<'_, str> {
    SPAN_TAG_RE.replace_all(input, "")
}

pub fn validate_runtime_context(
    values: &std::collections::HashMap<String, String>,
    label: &str,
) -> Result<(), String> {
    if values.len() > MAX_RUNTIME_VALUES {
        return Err(format!(
            "{label} contains {} values; the safety limit is {MAX_RUNTIME_VALUES}",
            values.len()
        ));
    }
    let mut total = 0usize;
    for (key, value) in values {
        if key.len() > 256 {
            return Err(format!("{label} key is longer than 256 bytes"));
        }
        if value.len() > MAX_RUNTIME_VALUE_BYTES {
            return Err(format!(
                "{label} value '{key}' exceeds the 1 MB safety limit"
            ));
        }
        total = total
            .checked_add(key.len())
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or_else(|| format!("{label} size overflow"))?;
        if total > MAX_RUNTIME_CONTEXT_BYTES {
            return Err(format!("{label} exceeds the 8 MB safety limit"));
        }
    }
    Ok(())
}

/// Reject profiles whose declared Cartesian products and retry policy can
/// create an unexpectedly large or costly run before any extraction or model
/// call starts.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct RunBudgetEstimate {
    pub step_units_upper_bound: u64,
    pub merge_calls_upper_bound: u64,
    pub provider_attempts_upper_bound: u64,
}

pub fn estimate_run_budget(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
) -> Result<RunBudgetEstimate, String> {
    let mut units = 0u64;
    let mut merge_calls = 0u64;
    for step in config.steps.iter().filter(|step| step.enabled) {
        let items = if step.phase == crate::pipeline_config::Phase::Parallel {
            step.for_each
                .as_ref()
                .map(|fan_out| u64::from(fan_out.max.max(1)))
                .unwrap_or(1)
        } else {
            1
        };
        let agents = if step.phase == crate::pipeline_config::Phase::Parallel {
            step.agents.len().max(1) as u64
        } else {
            1
        };
        units = units
            .checked_add(
                items
                    .checked_mul(agents)
                    .ok_or_else(|| "Run unit count overflow".to_string())?,
            )
            .ok_or_else(|| "Run unit count overflow".to_string())?;
        if config.merge.enabled && agents > 1 {
            merge_calls = merge_calls
                .checked_add(items)
                .ok_or_else(|| "Merge call count overflow".to_string())?;
        }
    }
    let attempts_per_step = u64::from(settings.max_retries).saturating_add(1);
    let mut attempts = units
        .checked_mul(attempts_per_step)
        // Shared slots normally warm only once. A timed-out warm-up can leave
        // its slot unprepared, however, so the safe upper bound is one warm-up
        // for every dispatched logical attempt.
        .and_then(|value| {
            if config.context_cache.enabled {
                units
                    .checked_mul(attempts_per_step)
                    .and_then(|warmups| value.checked_add(warmups))
            } else {
                Some(value)
            }
        })
        .and_then(|value| value.checked_add(merge_calls))
        // Reserve a few calls for extraction, orientation, and reconciliation.
        .and_then(|value| value.checked_add(4))
        .ok_or_else(|| "Provider attempt count overflow".to_string())?;
    // Any logical call can make one additional provider call after a durable
    // usage-limit failure when the user enables the cross-provider fallback.
    if settings.usage_limit_fallback_agent().is_some() {
        attempts = attempts
            .checked_mul(2)
            .ok_or_else(|| "Provider attempt count overflow".to_string())?;
    }
    Ok(RunBudgetEstimate {
        step_units_upper_bound: units,
        merge_calls_upper_bound: merge_calls,
        provider_attempts_upper_bound: attempts,
    })
}

pub fn validate_run_budget(
    config: &crate::pipeline_config::PipelineConfig,
    settings: &crate::settings::Settings,
) -> Result<(), String> {
    let estimate = estimate_run_budget(config, settings)?;
    if estimate.step_units_upper_bound > MAX_RUN_STEP_UNITS {
        return Err(format!(
            "Profile can dispatch {} step units; the safety limit is {MAX_RUN_STEP_UNITS}",
            estimate.step_units_upper_bound
        ));
    }
    if estimate.provider_attempts_upper_bound > MAX_RUN_PROVIDER_ATTEMPTS {
        return Err(format!(
            "Profile can make up to {} provider attempts; the safety limit is {MAX_RUN_PROVIDER_ATTEMPTS}",
            estimate.provider_attempts_upper_bound
        ));
    }
    Ok(())
}

/// Open a path without following a final-component symlink or blocking on a
/// FIFO/device. The post-open type check closes the remaining TOCTOU window.
pub fn open_regular_file(path: &Path) -> Result<fs::File, String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|error| format!("Failed to open regular file {}: {error}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(windows)]
    {
        // Reaching here with the reparse attribute set means a *file* reparse
        // point that is not a symlink (std reports symlinks and mount points
        // via is_symlink, rejected above; junctions are directories): in
        // practice a cloud placeholder such as OneDrive Files-On-Demand. The
        // handle was opened with FILE_FLAG_OPEN_REPARSE_POINT — precisely the
        // flag that tells the cloud filter NOT to hydrate — so reads of an
        // online-only file would fail. Reopen without the flag and let the
        // filter recall the content like any ordinary application. The
        // post-reopen type check keeps the anti-symlink property for the
        // reopened handle.
        use std::os::windows::fs::MetadataExt as _;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            drop(file);
            let reopened = fs::OpenOptions::new().read(true).open(path).map_err(|error| {
                format!(
                    "Failed to open {} (a cloud-placeholder file that could not be hydrated): {error}",
                    path.display()
                )
            })?;
            let metadata = reopened
                .metadata()
                .map_err(|error| format!("Failed to inspect {}: {error}", path.display()))?;
            if !metadata.file_type().is_file() {
                return Err(format!("{} is not a regular file", path.display()));
            }
            return Ok(reopened);
        }
    }
    Ok(file)
}

// What a test pretends this volume does with a flush: the error `F_FULLFSYNC`
// gives, and whether a plain `fsync` fails too.
#[cfg(test)]
thread_local! {
    static SIMULATED_SYNC: std::cell::Cell<Option<(i32, bool)>> = const { std::cell::Cell::new(None) };
}
// Whether a test pretends this volume invents its permission bits.
#[cfg(all(test, unix))]
thread_local! {
    static SIMULATED_INVENTED_PERMISSIONS: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}
#[cfg(all(test, unix))]
pub(crate) fn simulate_invented_permissions(invented: bool) {
    SIMULATED_INVENTED_PERMISSIONS.with(|simulated| simulated.set(invented));
}
#[cfg(all(test, unix))]
pub(crate) fn simulate_sync_failure(failure: Option<(i32, bool)>) {
    SIMULATED_SYNC.with(|simulated| simulated.set(failure));
}

fn full_sync(file: &fs::File) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some((code, _)) = SIMULATED_SYNC.with(std::cell::Cell::get) {
        return Err(std::io::Error::from_raw_os_error(code));
    }
    file.sync_all()
}

#[cfg(unix)]
fn plain_sync(file: &fs::File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd as _;
    #[cfg(test)]
    if let Some((code, true)) = SIMULATED_SYNC.with(std::cell::Cell::get) {
        return Err(std::io::Error::from_raw_os_error(code));
    }
    // SAFETY: the descriptor stays open through this synchronous call.
    match unsafe { libc::fsync(file.as_raw_fd()) } {
        0 => Ok(()),
        _ => Err(std::io::Error::last_os_error()),
    }
}

/// `File::sync_all` for any volume. On macOS that call is `F_FULLFSYNC`, which
/// network shares and devices can refuse though a plain `fsync` works, so a
/// refusal falls back to the strongest flush the volume has. A write error is
/// still an error. Use this before a rename; `sync_landed` is for after one.
pub fn sync_file(file: &fs::File) -> std::io::Result<()> {
    let result = full_sync(file);
    #[cfg(unix)]
    if let Err(error) = &result {
        let refused = [
            libc::ENOTSUP,
            libc::ENOTTY,
            libc::EINVAL,
            libc::ENODEV,
            libc::ENOSYS,
        ];
        if error
            .raw_os_error()
            .is_some_and(|code| refused.contains(&code))
        {
            return plain_sync(file);
        }
    }
    result
}

/// Flushes a file or folder whose write, rename, or create has already landed.
/// The change has happened by now, so a failed flush is logged and never
/// reported as a failure of the operation: the page would otherwise deny a move
/// that took place, or an export that sits in the chosen folder.
pub fn sync_landed(landed: &fs::File) {
    if let Err(error) = sync_file(landed) {
        eprintln!("Could not flush a change to its drive: {error}");
    }
}

/// Whether this file's volume keeps Unix permission bits. exFAT, FAT, NTFS, and
/// network shares make them up, usually with every file marked as a program, so
/// they say nothing there. An unknown volume counts as keeping them: a refused
/// open is the safe way to be wrong.
#[cfg(unix)]
pub fn volume_keeps_permissions(file: &fs::File) -> bool {
    use std::os::fd::AsRawFd as _;
    #[cfg(test)]
    if SIMULATED_INVENTED_PERMISSIONS.with(std::cell::Cell::get) {
        return false;
    }
    let mut volume = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: the descriptor stays open and the buffer stays valid for this call.
    if unsafe { libc::fstatfs(file.as_raw_fd(), volume.as_mut_ptr()) } != 0 {
        return true;
    }
    // SAFETY: fstatfs succeeded, so the buffer is initialized.
    let volume = unsafe { volume.assume_init() };
    !invented_permissions(&volume)
}

/// macOS names the volume type. `lifs` is its user-space driver for FAT and exFAT.
#[cfg(target_os = "macos")]
fn invented_permissions(volume: &libc::statfs) -> bool {
    // SAFETY: the kernel fills this field with a NUL-terminated name.
    let name = unsafe { std::ffi::CStr::from_ptr(volume.f_fstypename.as_ptr()) };
    matches!(
        name.to_bytes(),
        b"exfat" | b"lifs" | b"msdos" | b"ntfs" | b"smbfs" | b"webdav"
    )
}

/// Linux reports a magic number: FAT, exFAT, the two NTFS drivers, and SMB.
#[cfg(target_os = "linux")]
#[allow(clippy::unnecessary_cast)] // The field's width differs by platform.
fn invented_permissions(volume: &libc::statfs) -> bool {
    matches!(
        volume.f_type as u32,
        0x4d44 | 0x2011_bab0 | 0x5346_544e | 0x7366_746e | 0x517b | 0xff53_4d42 | 0xfe53_4d42
    )
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
fn invented_permissions(_volume: &libc::statfs) -> bool {
    false
}

/// Narrows a private path. A volume that keeps no permissions can refuse the
/// request, and has nothing to narrow, so only a volume that keeps them fails.
#[cfg(unix)]
pub fn restrict_permissions(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let result = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    match fs::File::open(path) {
        Ok(file) if !volume_keeps_permissions(&file) => Ok(()),
        _ => result,
    }
}

/// Replace every occurrence while checking the final allocation before it is
/// attempted. Chaining this helper keeps every intermediate string bounded.
pub fn replace_all_limited(
    input: &str,
    needle: &str,
    replacement: &str,
    limit: usize,
    label: &str,
) -> Result<String, String> {
    if needle.is_empty() || !input.contains(needle) {
        if input.len() > limit {
            return Err(format!(
                "{label} exceeds the {} MB safety limit",
                limit / 1024 / 1024
            ));
        }
        return Ok(input.to_string());
    }
    let count = input.match_indices(needle).count();
    let removed = needle
        .len()
        .checked_mul(count)
        .ok_or_else(|| format!("{label} size overflow"))?;
    let added = replacement
        .len()
        .checked_mul(count)
        .ok_or_else(|| format!("{label} size overflow"))?;
    let final_len = input
        .len()
        .checked_sub(removed)
        .and_then(|length| length.checked_add(added))
        .ok_or_else(|| format!("{label} size overflow"))?;
    if final_len > limit {
        return Err(format!(
            "{label} would expand to {} MB; the safety limit is {} MB",
            final_len / 1024 / 1024,
            limit / 1024 / 1024
        ));
    }
    let mut output = String::with_capacity(final_len);
    let mut rest = input;
    while let Some(offset) = rest.find(needle) {
        output.push_str(&rest[..offset]);
        output.push_str(replacement);
        rest = &rest[offset + needle.len()..];
    }
    output.push_str(rest);
    Ok(output)
}

pub fn push_str_limited(
    output: &mut String,
    value: &str,
    limit: usize,
    label: &str,
) -> Result<(), String> {
    if value.len() > limit.saturating_sub(output.len()) {
        return Err(format!(
            "{label} exceeds the {} MB safety limit",
            limit / 1024 / 1024
        ));
    }
    output.push_str(value);
    Ok(())
}

/// State shared by recursive walkers. It independently bounds files, empty
/// directories, and wall time so none can evade the other limits. A walk that
/// copies or hashes files is bounded by its bytes and takes as long as the disk
/// takes, so it drops the clock with [`WalkBudget::without_time_limit`].
pub struct WalkBudget {
    started: Instant,
    entries: usize,
    directories: usize,
    label: &'static str,
    cancellable: bool,
    timed: bool,
}

impl WalkBudget {
    pub fn new(label: &'static str) -> Self {
        Self {
            started: Instant::now(),
            entries: 0,
            directories: 1,
            label,
            cancellable: false,
            timed: true,
        }
    }

    pub fn new_cancellable(label: &'static str) -> Self {
        Self {
            cancellable: true,
            ..Self::new(label)
        }
    }

    pub fn without_time_limit(self) -> Self {
        Self {
            timed: false,
            ..self
        }
    }

    /// The same walk with its clock already past the time limit.
    #[cfg(test)]
    pub fn overdue(self) -> Self {
        Self {
            started: Instant::now() - MAX_WALK_TIME * 2,
            ..self
        }
    }

    pub fn entry(&mut self) -> Result<(), String> {
        self.entries = self.entries.saturating_add(1);
        self.check()
    }

    pub fn directory(&mut self) -> Result<(), String> {
        self.directories = self.directories.saturating_add(1);
        self.check()
    }

    fn check(&self) -> Result<(), String> {
        if self.entries > MAX_WALK_ENTRIES {
            return Err(format!("{} exceeded the entry safety limit", self.label));
        }
        if self.directories > MAX_WALK_DIRS {
            return Err(format!(
                "{} exceeded the directory safety limit",
                self.label
            ));
        }
        if self.timed && self.started.elapsed() > MAX_WALK_TIME {
            return Err(format!(
                "{} exceeded the scan-time safety limit",
                self.label
            ));
        }
        if self.cancellable && crate::commands::is_cancelled() {
            return Err("Pipeline cancelled".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file on this machine's temporary volume. Set `TMPDIR` to a folder on
    /// an exFAT drive to run these tests on one.
    #[cfg(unix)]
    fn scratch() -> (tempfile::TempDir, fs::File) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file");
        fs::write(&path, "text").unwrap();
        let file = fs::File::open(path).unwrap();
        (temp, file)
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_device_that_refuses_a_full_flush_gets_a_plain_one() {
        let device = fs::File::open("/dev/null").unwrap();
        assert!(device.sync_all().is_err(), "the case this guards");
        sync_file(&device).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_flush_falls_back_and_a_write_error_still_fails() {
        let (_temp, file) = scratch();
        for code in [libc::ENOTSUP, libc::ENOTTY, libc::EINVAL] {
            simulate_sync_failure(Some((code, false)));
            let result = sync_file(&file);
            simulate_sync_failure(None);
            result.unwrap();
        }
        simulate_sync_failure(Some((libc::ENOTSUP, true)));
        let refused_twice = sync_file(&file);
        simulate_sync_failure(Some((libc::EIO, false)));
        let write_error = sync_file(&file);
        simulate_sync_failure(None);
        assert_eq!(
            refused_twice.unwrap_err().raw_os_error(),
            Some(libc::ENOTSUP)
        );
        assert_eq!(write_error.unwrap_err().raw_os_error(), Some(libc::EIO));
    }

    #[cfg(unix)]
    #[test]
    fn this_volume_is_read_for_what_its_permissions_mean() {
        use std::os::unix::fs::PermissionsExt as _;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("kept");
        fs::write(&path, "text").unwrap();
        restrict_permissions(&path, 0o400).unwrap();
        let file = fs::File::open(&path).unwrap();
        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        if volume_keeps_permissions(&file) {
            assert_eq!(mode, 0o400);
        }
        assert!(restrict_permissions(&temp.path().join("gone"), 0o400).is_err());
    }

    /// `sync_all` is refused on shares where a plain flush works, so an export
    /// or a saved snapshot must not stop on it.
    #[test]
    fn exports_and_snapshots_flush_in_a_way_any_volume_accepts() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        for path in [
            "workbench/commands/project.rs",
            "workbench/commands/research.rs",
            "commands/export/formats.rs",
            "workbench/research/papers.rs",
            "orchestration/durable.rs",
            "workbench/programs/delivery.rs",
            "workbench/programs/capsule.rs",
            "workbench/release/exchange/export.rs",
            "workbench/shell.rs",
            "workbench/tool_setup.rs",
            "workbench/release/archive/export.rs",
            "commands/reuse_artifacts.rs",
            "commands/run_storage.rs",
            "workbench/project/files.rs",
            "runs/persistence.rs",
            "runs/inspection.rs",
            "runs/retention.rs",
            "runs/trash.rs",
            "pipeline/executor/checkpoints.rs",
            "pipeline/response_journal.rs",
            "pipeline/api_common/tool_files.rs",
        ] {
            let text = fs::read_to_string(source.join(path)).unwrap();
            assert!(!text.contains("sync_all()"), "{path}");
        }
    }

    /// Exclusive renames fail on exFAT, and these writes are content-addressed,
    /// so a rename over an existing file changes nothing.
    #[test]
    fn workspace_storage_never_needs_an_exclusive_rename() {
        fn visit(directory: &Path, found: &mut Vec<String>) {
            for entry in fs::read_dir(directory).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, found);
                } else if fs::read_to_string(&path)
                    .is_ok_and(|text| text.contains(".persist_noclobber("))
                {
                    found.push(path.display().to_string());
                }
            }
        }
        let mut found = Vec::new();
        visit(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src/workbench"),
            &mut found,
        );
        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn a_walk_without_a_time_limit_keeps_its_other_limits() {
        let overdue = |walk: WalkBudget| WalkBudget {
            started: Instant::now() - MAX_WALK_TIME * 2,
            ..walk
        };
        let mut timed = overdue(WalkBudget::new("copy"));
        assert!(timed.entry().unwrap_err().contains("scan-time"));
        let mut untimed = overdue(WalkBudget::new("copy").without_time_limit());
        assert!(untimed.entry().is_ok());
        untimed.entries = MAX_WALK_ENTRIES;
        assert!(untimed.entry().unwrap_err().contains("entry safety limit"));
    }

    #[test]
    fn replacement_rejects_multiplicative_expansion_before_allocating() {
        let input = "{x}".repeat(100);
        let error =
            replace_all_limited(&input, "{x}", &"y".repeat(100), 1_000, "prompt").unwrap_err();
        assert!(error.contains("expand"));
    }

    #[test]
    fn replacement_preserves_normal_text() {
        assert_eq!(
            replace_all_limited("a {x} b {x}", "{x}", "ok", 100, "prompt").unwrap(),
            "a ok b ok"
        );
    }

    #[test]
    fn span_cleanup_preserves_text_and_non_span_markup() {
        let input = r#"<SPAN id="equation>1">Estimate</SPAN> and <span class='x'>evidence</span> <div>kept</div>"#;
        assert_eq!(
            strip_span_tags(input),
            "Estimate and evidence <div>kept</div>"
        );
    }

    #[test]
    fn run_budget_rejects_large_agent_fan_out_products() {
        let step = crate::pipeline_config::StepConfig {
            id: "fan-out".to_string(),
            enabled: true,
            phase: crate::pipeline_config::Phase::Parallel,
            agents: vec![
                "claude".to_string(),
                "codex".to_string(),
                "antigravity".to_string(),
            ],
            for_each: Some(crate::pipeline_config::ForEach {
                glob: "**/*".to_string(),
                max: 1_000,
                artifact: None,
            }),
            ..Default::default()
        };
        let config = crate::pipeline_config::PipelineConfig {
            steps: vec![step],
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        assert!(validate_run_budget(&config, &crate::settings::Settings::default()).is_err());
    }

    #[test]
    fn run_budget_accounts_for_shared_context_warmups() {
        let step = crate::pipeline_config::StepConfig {
            id: "fan-out".to_string(),
            enabled: true,
            phase: crate::pipeline_config::Phase::Parallel,
            agents: vec![
                "claude".to_string(),
                "codex".to_string(),
                "antigravity".to_string(),
                "local".to_string(),
            ],
            for_each: Some(crate::pipeline_config::ForEach {
                glob: "**/*".to_string(),
                max: 5,
                artifact: None,
            }),
            ..Default::default()
        };
        let mut config = crate::pipeline_config::PipelineConfig {
            steps: (0..100).map(|_| step.clone()).collect(),
            merge: Default::default(),
            outputs: Default::default(),
            context_cache: Default::default(),
            use_orientation: true,
            orientation_prompt: String::new(),
            orientation_schema: None,
            extraction: Default::default(),
            parallel_context_template: String::new(),
            variables: Vec::new(),
        };
        let settings = crate::settings::Settings {
            max_retries: 1,
            ..Default::default()
        };
        assert!(validate_run_budget(&config, &settings).is_ok());
        config.context_cache.enabled = true;
        let error = validate_run_budget(&config, &settings).unwrap_err();
        assert!(error.contains("provider attempts"), "{error}");
    }
}
