//! Process-tree ownership. Review scopes and independent children remain separate.
#[cfg(windows)]
pub(crate) static CHILD_JOBS: std::sync::Mutex<Vec<(u32, usize)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(windows)]
mod windows_spawn;

/// Minimal Win32 Job Object bindings. Keeping them here avoids making the
/// platform-neutral build depend on a large Windows bindings crate.
#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;

    type Handle = *mut c_void;
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[repr(C)]
    #[derive(Default)]
    struct BasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimitInformation {
        basic_limit_information: BasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            info_class: i32,
            info: *const c_void,
            length: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, exit_code: u32) -> i32;
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
        fn CloseHandle(handle: Handle) -> i32;
    }

    pub fn assign(process_id: u32) -> std::io::Result<usize> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let mut info = ExtendedLimitInformation::default();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                (&info as *const ExtendedLimitInformation).cast(),
                std::mem::size_of::<ExtendedLimitInformation>() as u32,
            ) == 0
            {
                let error = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(error);
            }
            let process = OpenProcess(
                PROCESS_TERMINATE | PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                process_id,
            );
            if process.is_null() || AssignProcessToJobObject(job, process) == 0 {
                let error = std::io::Error::last_os_error();
                if !process.is_null() {
                    CloseHandle(process);
                }
                CloseHandle(job);
                return Err(error);
            }
            CloseHandle(process);
            Ok(job as usize)
        }
    }

    pub fn terminate(handle: usize) {
        unsafe {
            let job = handle as Handle;
            TerminateJobObject(job, 1);
            CloseHandle(job);
        }
    }

    pub fn close(handle: usize) {
        unsafe {
            CloseHandle(handle as Handle);
        }
    }
}

#[cfg(windows)]
pub(crate) fn register_process_job(pid: u32) {
    let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
    if jobs.iter().any(|(registered, _)| *registered == pid) {
        return;
    }
    if let Ok(handle) = windows_job::assign(pid) {
        jobs.push((pid, handle));
    }
}

/// Research launchers may fork and exit immediately. On Windows create them
/// suspended, capture their job, and only then let their first thread run.
pub(crate) fn spawn_independent(
    command: &mut std::process::Command,
) -> std::io::Result<std::process::Child> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000 | 0x00000004); // NO_WINDOW | SUSPENDED
        let mut child = command.spawn()?;
        let pid = child.id();
        let setup = windows_job::assign(pid).and_then(|handle| {
            CHILD_JOBS
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((pid, handle));
            windows_spawn::resume(pid)
        });
        if let Err(error) = setup {
            // Nothing is allowed to run without reliable tree ownership.
            let _ = child.kill();
            let _ = child.wait();
            unregister_process_job(pid);
            return Err(std::io::Error::new(
                error.kind(),
                format!("Could not establish Windows process cleanup: {error}"),
            ));
        }
        Ok(child)
    }
    #[cfg(not(windows))]
    command.spawn()
}

#[cfg(not(windows))]
pub(crate) fn register_process_job(_pid: u32) {}

#[cfg(windows)]
pub(crate) fn unregister_process_job(pid: u32) {
    let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
    let handles: Vec<usize> = jobs
        .iter()
        .filter(|(registered, _)| *registered == pid)
        .map(|(_, handle)| *handle)
        .collect();
    jobs.retain(|(registered, _)| *registered != pid);
    drop(jobs);
    for handle in handles {
        windows_job::close(handle);
    }
}

#[cfg(not(windows))]
pub(crate) fn unregister_process_job(_pid: u32) {}

/// Register a child process PID so it can be killed on cancel. Also attributes
/// the PID to the current pass (if any) for per-pass cancellation.
pub fn register_child_pid(pid: u32) {
    // Callers already skip pid 0 (spawn without a real id), but guard here
    // too so kill_all_children never signals pid 0 / process group 0.
    if pid == 0 {
        return;
    }
    register_process_job(pid);
    crate::run_scope::current().register_child(pid, crate::pipeline::logging::current_pass());
}

/// Unregister a child process PID after it exits.
pub fn unregister_child_pid(pid: u32) {
    crate::run_scope::forget_child_everywhere(pid);
    unregister_process_job(pid);
}

/// Register a managed-engine installer child for Windows job-object tracking
/// only. Installs run outside any pipeline run and own their kill path
/// (`engines::cancel_install`); they must never enter a run scope, where the
/// end or cancellation of an unrelated run would SIGKILL a half-finished
/// multi-gigabyte install mid-download.
pub(crate) fn register_engine_child_pid(pid: u32) {
    if pid == 0 {
        return;
    }
    register_process_job(pid);
}

/// Counterpart to `register_engine_child_pid` once the installer child exits.
pub(crate) fn unregister_engine_child_pid(pid: u32) {
    unregister_process_job(pid);
}

/// Register an independently owned long-lived child for Windows job-object
/// tracking without adding it to Pipeline's run cancellation registry.
pub(crate) fn register_independent_child_pid(pid: u32) {
    if pid > 0 {
        register_process_job(pid);
    }
}

/// Release independent process-tree tracking after the owner has reaped it.
pub(crate) fn unregister_independent_child_pid(pid: u32) {
    if pid > 0 {
        unregister_process_job(pid);
    }
}

/// Force-stop an independently owned process tree. It is deliberately absent
/// from every run scope, so ordinary Pipeline cancellation cannot reach it.
pub(crate) fn kill_independent_process(pid: u32) {
    kill_process(pid);
}

/// Terminate subprocesses attributed to one pass without marking the pass as
/// user-cancelled. Used when the whole logical provider call reaches its
/// deadline, so retry policy can still decide what happens next.
pub(crate) fn kill_pass_children(pass_key: &str) {
    crate::run_scope::current().kill_pass_children(pass_key);
}

/// Kill every registered child process of the current run.
pub(crate) fn kill_all_children() {
    crate::run_scope::current().kill_children();
}

pub(crate) fn kill_process(pid: u32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    {
        // Validate PID fits in a positive i32 to prevent overflow when negating
        if pid > i32::MAX as u32 {
            return;
        }
        let pid_i32 = pid as i32;
        // Cancellation is a force-stop operation. Signal immediately while
        // the PID is still registered as owned by Pipeline; never schedule a
        // later PID-only escalation, because the child may be reaped and that
        // numeric PID/process-group ID reused by an unrelated process.
        unsafe {
            libc::kill(-pid_i32, libc::SIGKILL);
            libc::kill(pid_i32, libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let job = {
            let mut jobs = CHILD_JOBS.lock().unwrap_or_else(|e| e.into_inner());
            let handle = jobs
                .iter()
                .find(|(registered, _)| *registered == pid)
                .map(|(_, handle)| *handle);
            jobs.retain(|(registered, _)| *registered != pid);
            handle
        };
        if let Some(handle) = job {
            windows_job::terminate(handle);
            return;
        }
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(0x08000000)
            .status();
    }
}
