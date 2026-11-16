//! macOS cleanup independent of parent PIDs, process groups, and sessions.
//!
//! Each command's Seatbelt profile grants lookup of one random, unregistered
//! Mach name and denies a sibling control name. Only that inherited policy
//! allows the first and denies the second: unsandboxed hosts allow both, other
//! commands deny both. No service is registered. The start gate stays closed
//! until the kernel confirms this signature on the child and rejects the host.
//! Seatbelt refuses sandbox replacement once applied; fork, exec, setsid, and
//! POSIX_SPAWN_SETSID all retain the signature. No launch syscall is blocked.
//!
//! Cleanup scans complete process snapshots until no matching live process
//! remains, including reparented grandchildren and children with closed pipes.
//! Queries and signals use PID-version audit tokens to avoid PID-reuse races.
//! Zombies cannot execute and are left for their actual parent to reap. A
//! bounded deadline reports failure rather than silently accepting survivors.
use mxc_sdk::v1::{ContainerRequest, Containment};
use std::{
    ffi::CString,
    io,
    time::{Duration, Instant},
};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct AuditToken {
    val: [u32; 8],
}

#[repr(C)]
#[derive(Default)]
struct UniqueIdentifier {
    uuid: [u8; 16],
    unique_id: u64,
    parent_unique_id: u64,
    id_version: i32,
    original_parent_id_version: i32,
    reserved: [u64; 2],
}

unsafe extern "C" {
    static SANDBOX_CHECK_NO_REPORT: libc::c_int;
    fn sandbox_check_by_audit_token(
        token: AuditToken,
        operation: *const libc::c_char,
        filter: libc::c_int,
        ...
    ) -> libc::c_int;
    fn proc_signal_with_audittoken(token: *mut AuditToken, signal: libc::c_int) -> libc::c_int;
}

pub(in super::super) struct Tree {
    member: CString,
    control: CString,
}

impl Tree {
    pub(in super::super) fn prepare(request: &mut ContainerRequest) -> Result<Self, String> {
        let Containment::Seatbelt(config) = &mut request.containment else {
            return Err("Missing macOS command sandbox".into());
        };
        let profile = config
            .profile_override
            .as_mut()
            .ok_or("Missing macOS command policy")?;
        let mut random = [0u8; 32];
        getrandom::fill(&mut random)
            .map_err(|e| format!("Cannot identify command sandbox: {e}"))?;
        let nonce: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let name = format!("org.pipeline.command.{nonce}");
        let tree = Self {
            member: CString::new(format!("{name}.member")).unwrap(),
            control: CString::new(format!("{name}.control")).unwrap(),
        };
        profile.push_str(&format!(
            "(allow mach-lookup (global-name \"{name}.member\"))\n\
             (deny mach-lookup (global-name \"{name}.control\") (with no-log))\n"
        ));
        Ok(tree)
    }

    pub(in super::super) fn verify(&self, pid: u32) -> Result<(), String> {
        let verify = || -> io::Result<Option<bool>> {
            let child = identity(pid as libc::pid_t)?;
            // SAFETY: getpid has no preconditions.
            let host = identity(unsafe { libc::getpid() })?;
            let pids = list_pids()?;
            let verified = match (child, host) {
                (Some(child), Some(host)) => {
                    pids.contains(&(pid as libc::pid_t))
                        && self.contains(child)?
                        && !self.contains(host)?
                }
                _ => false,
            };
            // The SDK starts /bin/sh, which can exec the gated Bash shell
            // while we query its sandbox. Exec changes the audit-token
            // version: a stale token is not evidence of missing ownership.
            // Retry only that transition, never a stable policy mismatch.
            if identity(pid as libc::pid_t)? != child {
                return Ok(None);
            }
            Ok(Some(verified))
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match verify().map_err(|e| format!("Cannot verify command cleanup: {e}"))? {
                Some(true) => return Ok(()),
                None if Instant::now() < deadline => std::thread::yield_now(),
                _ => break,
            }
        }
        Err("macOS could not establish command process ownership; the command did not run".into())
    }

    pub(in super::super) fn terminate(&self) -> Result<(), String> {
        self.terminate_inner()
            .map_err(|e| format!("Could not stop all command processes: {e}"))
    }

    fn terminate_inner(&self) -> io::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut quiet = 0;
        loop {
            let mut unsettled = false;
            for pid in list_pids()? {
                if pid <= 0 {
                    continue;
                }
                let Some(mut token) = identity(pid)? else {
                    unsettled = true;
                    continue;
                };
                let owned = self.contains(token)?;
                // Exec changes the audit-token version too. A stale query is
                // not proof that the process left this command. Rescan after
                // exits or identity changes, including concurrent reparenting.
                if identity(pid)? != Some(token) {
                    unsettled = true;
                    continue;
                }
                if !owned || !is_live(pid)? {
                    continue;
                }
                unsettled = true;
                // SAFETY: libproc atomically checks PID and PID version before
                // signaling. An exited process's replacement is never targeted.
                let error = unsafe { proc_signal_with_audittoken(&mut token, libc::SIGKILL) };
                if error != 0 && error != libc::ESRCH {
                    return Err(io::Error::from_raw_os_error(error));
                }
            }
            quiet = if unsettled { 0 } else { quiet + 1 };
            if quiet == 2 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "command descendants survived termination",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn contains(&self, token: AuditToken) -> io::Result<bool> {
        // Check the denial first so an unsandboxed process never qualifies.
        Ok(self.lookup(token, &self.control)? == Some(false)
            && self.lookup(token, &self.member)? == Some(true))
    }

    fn lookup(&self, token: AuditToken, name: &CString) -> io::Result<Option<bool>> {
        // SANDBOX_FILTER_GLOBAL_NAME is 2 (also used by WebKit's SandboxSPI).
        // SAFETY: the token is copied by value; both strings are NUL-terminated.
        // The no-report flag suppresses logs for these permission-only probes.
        let result = unsafe {
            sandbox_check_by_audit_token(
                token,
                c"mach-lookup".as_ptr(),
                2 | SANDBOX_CHECK_NO_REPORT,
                name.as_ptr(),
            )
        };
        match result {
            0 => Ok(Some(true)),
            1 => Ok(Some(false)),
            _ => {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn identity(pid: libc::pid_t) -> io::Result<Option<AuditToken>> {
    let mut info = UniqueIdentifier::default();
    if !pid_info(pid, 17, &mut info)? {
        return Ok(None);
    }
    let mut token = AuditToken { val: [0; 8] };
    token.val[5] = pid as u32;
    token.val[7] = info.id_version as u32;
    Ok(Some(token))
}

fn is_live(pid: libc::pid_t) -> io::Result<bool> {
    // SAFETY: proc_bsdinfo consists entirely of integers and byte arrays.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    Ok(pid_info(pid, libc::PROC_PIDTBSDINFO, &mut info)? && info.pbi_status != libc::SZOMB)
}

fn pid_info<T>(pid: libc::pid_t, flavor: libc::c_int, info: &mut T) -> io::Result<bool> {
    let size = std::mem::size_of::<T>() as libc::c_int;
    // SAFETY: all callers use the fixed-layout record specified by `flavor`.
    // libproc writes at most `size` bytes to the supplied initialized record.
    let copied =
        unsafe { libc::proc_pidinfo(pid, flavor, 0, std::ptr::from_mut(info).cast(), size) };
    if copied == size {
        Ok(true)
    } else if copied == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(false)
        } else {
            Err(error)
        }
    } else {
        Err(io::Error::other(
            "macOS returned an incomplete process identity",
        ))
    }
}

fn list_pids() -> io::Result<Vec<libc::pid_t>> {
    // Retry a full buffer: a truncated snapshot cannot prove cleanup complete.
    // SAFETY: a null buffer requests the current process count.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut capacity = count as usize + 128;
    loop {
        if capacity > 1_048_576 {
            return Err(io::Error::other("macOS process list is too large"));
        }
        let mut pids = vec![0; capacity];
        // SAFETY: the vector is initialized and writable for the specified size.
        let listed = unsafe {
            libc::proc_listallpids(
                pids.as_mut_ptr().cast(),
                (capacity * std::mem::size_of::<libc::pid_t>()) as libc::c_int,
            )
        };
        if listed < 0 {
            return Err(io::Error::last_os_error());
        }
        if (listed as usize) < capacity {
            pids.truncate(listed as usize);
            // SAFETY: getpid has no preconditions. Our live host must appear
            // in every complete snapshot; an empty/restricted list is unsafe.
            if !pids.contains(&unsafe { libc::getpid() }) {
                return Err(io::Error::other(
                    "macOS returned an incomplete process list",
                ));
            }
            return Ok(pids);
        }
        capacity *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    #[test]
    #[ignore = "requires the real macOS sandbox"]
    fn native_macos_ownership_is_checked_before_releasing_the_start_gate() {
        let root = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let base = super::super::super::policy::request(
            root.path(),
            true,
            scratch.path(),
            "echo escaped > escaped",
            None,
        )
        .unwrap();
        let mut marked = base.clone();
        let tree = Tree::prepare(&mut marked).unwrap();
        marked.command = marked.command.replace(
            "IFS= read -r pipeline_gate;",
            "echo pipeline-gate-ready; IFS= read -r pipeline_gate;",
        );
        for _ in 0..16 {
            let mut child = mxc_sdk::v1::spawn(marked.clone(), Default::default()).unwrap();
            // Verify immediately, including while /bin/sh execs Bash.
            let verified = tree.verify(child.id());
            if let Err(error) = verified {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("{error}");
            }
            // The token-specific assertions need the final gated shell's
            // identity; another legitimate exec would invalidate that token.
            let mut ready = String::new();
            std::io::BufReader::new(child.take_stdout().unwrap())
                .read_line(&mut ready)
                .unwrap();
            assert_eq!(ready.trim(), "pipeline-gate-ready");
            let token = identity(child.id() as libc::pid_t).unwrap().unwrap();
            assert!(tree.contains(token).unwrap());
            let mut stale = token;
            stale.val[7] = stale.val[7].wrapping_add(1);
            assert!(!tree.contains(stale).unwrap());
            child.kill().unwrap();
            child.wait().unwrap();
            assert!(!root.path().join("escaped").exists());
        }
        // This child did not inherit the signature. Refuse it while the gate
        // is still closed; never infer ownership from the PID or parent alone.
        let mut unmarked = mxc_sdk::v1::spawn(base, Default::default()).unwrap();
        assert!(tree.verify(unmarked.id()).is_err());
        unmarked.kill().unwrap();
        unmarked.wait().unwrap();
        assert!(!root.path().join("escaped").exists());
    }
}
