//! Resume a newly created, suspended process after Job Object assignment.
//! Stable Rust doesn't expose Child's initial thread handle, so find that
//! thread using the documented Tool Help API while it is still suspended.
use std::ffi::c_void;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

type Handle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    id: u32,
    owner: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
    fn OpenThread(access: u32, inherit: i32, id: u32) -> Handle;
    fn GetProcessIdOfThread(thread: Handle) -> u32;
    fn ResumeThread(thread: Handle) -> u32;
}

pub(super) fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: snapshot and thread handles are checked before being owned;
    // all pointers refer to initialized storage of the documented ABI size.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(0x00000004, 0); // SNAPTHREAD
        if snapshot == -1isize as Handle {
            return Err(io::Error::last_os_error());
        }
        let snapshot = OwnedHandle::from_raw_handle(snapshot);
        let mut entry = ThreadEntry {
            size: std::mem::size_of::<ThreadEntry>() as u32,
            ..Default::default()
        };
        if Thread32First(snapshot.as_raw_handle(), &mut entry) == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut initial = None;
        loop {
            if entry.owner == pid {
                // Before the first instruction a fresh process has one thread.
                // Refuse an unexpected state rather than resume a guessed one.
                if initial.replace(entry.id).is_some() {
                    return Err(io::Error::other(
                        "New process has more than one initial thread",
                    ));
                }
            }
            entry.size = std::mem::size_of::<ThreadEntry>() as u32;
            if Thread32Next(snapshot.as_raw_handle(), &mut entry) == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(18) {
                    // ERROR_NO_MORE_FILES
                    return Err(error);
                }
                break;
            }
        }
        let id = initial
            .ok_or_else(|| io::Error::other("New process's initial thread was not found"))?;
        let thread = OpenThread(0x0002 | 0x0800, 0, id); // SUSPEND_RESUME | QUERY_LIMITED_INFORMATION
        if thread.is_null() {
            return Err(io::Error::last_os_error());
        }
        let thread = OwnedHandle::from_raw_handle(thread);
        if GetProcessIdOfThread(thread.as_raw_handle()) != pid {
            return Err(io::Error::other("New process's thread identity changed"));
        }
        match ResumeThread(thread.as_raw_handle()) {
            1 => Ok(()),
            u32::MAX => Err(io::Error::last_os_error()),
            _ => Err(io::Error::other(
                "New process's thread was not suspended exactly once",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const HELPER: &str = "commands::lifecycle::windows_spawn::tests::tree_helper";

    #[test]
    fn tree_helper() {
        let Ok(role) = std::env::var("PIPELINE_WINDOWS_TREE_HELPER") else {
            return;
        };
        let path = std::env::var_os("PIPELINE_WINDOWS_TREE_READY").unwrap();
        if role == "child" {
            std::fs::write(&path, std::process::id().to_string()).unwrap();
            std::thread::sleep(Duration::from_secs(60));
        } else {
            // The enclosing Job Object test deliberately kills this parent
            // and its still-running child together; waiting would defeat it.
            #[expect(
                clippy::zombie_processes,
                reason = "fixture verifies whole-job termination"
            )]
            let _child = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", HELPER, "--nocapture"])
                .env("PIPELINE_WINDOWS_TREE_HELPER", "child")
                .spawn()
                .unwrap();
            let start = Instant::now();
            while !std::path::Path::new(&path).exists() {
                assert!(start.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            }
            // Exit while the helper still owns an inherited stdout pipe.
        }
    }

    #[test]
    fn a_registered_job_reaps_descendants_after_the_launcher_exits() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("child.pid");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", HELPER, "--nocapture"])
            .env("PIPELINE_WINDOWS_TREE_HELPER", "parent")
            .env("PIPELINE_WINDOWS_TREE_READY", &marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = super::super::spawn_independent(&mut command).unwrap();
        struct Cleanup(u32);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                super::super::kill_independent_process(self.0);
                super::super::unregister_independent_child_pid(self.0);
            }
        }
        let cleanup = Cleanup(child.id());
        // A second registration must not replace or lose the original job.
        super::super::register_independent_child_pid(child.id());
        let mut pipe = child.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut output = Vec::new();
            let _ = tx.send(pipe.read_to_end(&mut output));
        });
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
        assert!(rx.try_recv().is_err(), "helper should still hold the pipe");
        drop(cleanup);
        rx.recv_timeout(Duration::from_secs(5))
            .expect("helper survived its launcher job")
            .unwrap();
        reader.join().unwrap();
    }
}
