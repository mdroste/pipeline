//! Real macOS process tests run outside the enclosing test-runner sandbox.
use super::super::{policy, process, run};
use serde_json::json;
use std::{path::Path, time::Duration};

fn python(script: &str) -> String {
    format!("/usr/bin/python3 -c '{}'", script.replace('\'', "'\\''"))
}

async fn ready(
    root: &Path,
    names: &[&str],
    task: &tokio::task::JoinHandle<Result<serde_json::Value, String>>,
) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while names.iter().any(|name| !root.join(name).exists()) {
            assert!(
                !task.is_finished(),
                "Command exited before all children started"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires the real macOS sandbox and system Python"]
async fn native_macos_python_and_subprocess_launches_still_work() {
    let root = tempfile::tempdir().unwrap();
    let script = r#"
import os, subprocess, sys
for options in ({}, {'start_new_session': True}, {'preexec_fn': os.setsid}):
    result = subprocess.run([sys.executable, '-c', "print('José 日本語 αβ')"], capture_output=True, text=True, check=True, **options)
    assert result.stdout.strip() == 'José 日本語 αβ'
pid = os.posix_spawn('/bin/sh', ['sh', '-c', 'exit 7'], dict(os.environ), setsid=True)
assert os.waitpid(pid, 0)[1] == 7 << 8
assert subprocess.check_output('printf shell-ok', shell=True) == b'shell-ok'
# A descendant cannot replace the inherited ownership policy.
r = subprocess.run(['/usr/bin/sandbox-exec', '-p', '(version 1)(allow default)', '/bin/echo', 'escaped'], capture_output=True)
assert r.returncode != 0 and b'escaped' not in r.stdout
print('python-and-subprocess-ok', flush=True)
"#;
    let result = run(root.path(), true, &[], false, &python(script))
        .await
        .unwrap();
    assert_eq!(result["exitCode"], json!(0), "{result}");
    assert_eq!(result["stdout"], "python-and-subprocess-ok\n", "{result}");
}

#[derive(Clone, Copy, Debug)]
enum End {
    Completion,
    Cancellation,
    Timeout,
}

#[tokio::test]
#[ignore = "requires the real macOS sandbox and system Python"]
async fn native_macos_detached_children_stop_on_completion_cancellation_and_timeout() {
    for end in [End::Completion, End::Cancellation, End::Timeout] {
        let root = tempfile::tempdir().unwrap();
        // Each child announces itself before the foreground process can exit.
        // The original reproducer closed both output pipes. Keep one pair open
        // too, to prove cleanup closes inherited pipes without losing output.
        let child = r#"
import os, sys, time
name = sys.argv[1]
if name == 'double-fork':
    if os.fork(): os._exit(0)
    os.setsid()
    if os.fork(): os._exit(0)
if name != 'inherited':
    os.close(1)
    os.close(2)
with open(name + '.tmp', 'w') as f: f.write(str(os.getpid()))
os.replace(name + '.tmp', name + '.ready')
time.sleep(6)
with open(name + '.escaped', 'w') as f: f.write('survived')
"#;
        std::fs::write(root.path().join("child.py"), child).unwrap();
        let pause = if matches!(end, End::Completion) {
            ""
        } else {
            "time.sleep(30)"
        };
        let script = format!(
            r#"
import os, sys, time, subprocess
for name in ['inherited', 'closed', 'double-fork']:
    os.posix_spawn(sys.executable, [sys.executable, 'child.py', name], dict(os.environ), setsid=True)
subprocess.Popen([sys.executable, 'child.py', 'subprocess'], start_new_session=True)
while not all(os.path.exists(name + '.ready') for name in ['inherited', 'closed', 'double-fork', 'subprocess']): time.sleep(0.01)
print('foreground-ready', flush=True)
{pause}
"#
        );
        let path = root.path().to_path_buf();
        let task = tokio::spawn(async move {
            let scratch = tempfile::tempdir().unwrap();
            let mut request =
                policy::request(&path, true, scratch.path(), &python(&script), None).unwrap();
            if matches!(end, End::Timeout) {
                request.timeout_ms = Some(3000);
            }
            process::run(request, scratch, None).await
        });
        let started = std::time::Instant::now();
        if matches!(end, End::Cancellation) {
            ready(
                root.path(),
                &[
                    "inherited.ready",
                    "closed.ready",
                    "double-fork.ready",
                    "subprocess.ready",
                ],
                &task,
            )
            .await;
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        } else {
            let result = tokio::time::timeout(Duration::from_secs(10), task)
                .await
                .unwrap()
                .unwrap();
            match end {
                End::Completion => {
                    let result = result.unwrap();
                    assert_eq!(result["exitCode"], json!(0), "{result}");
                    assert_eq!(result["stdout"], "foreground-ready\n", "{result}");
                }
                End::Timeout => assert!(result.unwrap_err().contains("timed out after 3 seconds")),
                End::Cancellation => unreachable!(),
            }
        }
        for name in ["inherited", "closed", "double-fork", "subprocess"] {
            let pid: libc::pid_t =
                std::fs::read_to_string(root.path().join(format!("{name}.ready")))
                    .unwrap()
                    .parse()
                    .unwrap();
            assert!(!live(pid), "{end:?}: {name} process {pid} survived cleanup");
        }
        // Keep the disposable root around beyond the attempted write. This
        // catches late activity as well as checking that the PIDs stopped.
        tokio::time::sleep(Duration::from_secs(7).saturating_sub(started.elapsed())).await;
        for name in ["inherited", "closed", "double-fork", "subprocess"] {
            assert!(
                !root.path().join(format!("{name}.escaped")).exists(),
                "{end:?}: {name} wrote after cleanup"
            );
        }
    }
}

fn live(pid: libc::pid_t) -> bool {
    // SAFETY: proc_bsdinfo is all integers and byte arrays, and libproc receives
    // a valid buffer of exactly its size. Zombies cannot run or write files.
    unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of_val(&info) as libc::c_int;
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            std::ptr::from_mut(&mut info).cast(),
            size,
        ) == size
            && info.pbi_status != libc::SZOMB
    }
}

#[tokio::test]
#[ignore = "requires the real macOS sandbox"]
async fn native_macos_cleanup_preserves_other_commands_and_host_children() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    let peer = tokio::spawn(async move {
        run(
            &path,
            true,
            &[],
            false,
            "echo ready > peer.ready; sleep 3; echo peer-finished",
        )
        .await
    });
    let mut host = std::process::Command::new("/bin/sleep")
        .arg("5")
        .spawn()
        .unwrap();
    ready(root.path(), &["peer.ready"], &peer).await;
    let result = run(root.path(), true, &[], false, "echo victim-finished").await;
    let host_alive = host.try_wait().unwrap().is_none();
    let _ = host.kill();
    let _ = host.wait();
    assert!(host_alive, "Cleanup stopped an unrelated host child");
    assert_eq!(result.unwrap()["stdout"], "victim-finished\n");
    assert_eq!(peer.await.unwrap().unwrap()["stdout"], "peer-finished\n");
}

#[tokio::test]
#[ignore = "requires the real macOS sandbox and system Python"]
async fn native_macos_cleanup_handles_concurrent_forks_and_execs() {
    let root = tempfile::tempdir().unwrap();
    // Finite work even if cleanup regresses: at most 40 children, each exits
    // after three seconds. Re-exec changes the audit-token version repeatedly.
    std::fs::write(root.path().join("churn.py"), r#"
import os, sys, time
if len(sys.argv) > 1:
    deadline = float(sys.argv[1])
    with open(str(os.getpid()) + '.pid', 'w') as f: f.write(str(os.getpid()))
    if time.monotonic() < deadline:
        os.execv(sys.executable, [sys.executable, 'churn.py', str(deadline)])
    open(str(os.getpid()) + '.escaped', 'w').write('survived')
else:
    for i in range(40):
        os.posix_spawn(sys.executable, [sys.executable, 'churn.py', str(time.monotonic() + 3)], dict(os.environ), setsid=True)
        if i == 2: open('worker.ready', 'w').close()
        time.sleep(0.01)
"#).unwrap();
    let script = r#"
import os, sys, time
os.posix_spawn(sys.executable, [sys.executable, 'churn.py'], dict(os.environ), setsid=True, file_actions=[(os.POSIX_SPAWN_OPEN, 1, '/dev/null', os.O_WRONLY, 0), (os.POSIX_SPAWN_OPEN, 2, '/dev/null', os.O_WRONLY, 0)])
while not (os.path.exists('worker.ready') and any(name.endswith('.pid') for name in os.listdir('.'))): time.sleep(0.005)
print('churn-started', flush=True)
"#;
    let result = run(root.path(), true, &[], false, &python(script))
        .await
        .unwrap();
    assert_eq!(result["exitCode"], json!(0), "{result}");
    assert_eq!(result["stdout"], "churn-started\n");
    let pids: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pid"))
        .collect();
    assert!(!pids.is_empty(), "No exec loop started");
    for path in pids {
        // Read the filename: SIGKILL can interrupt the file's write.
        let pid: libc::pid_t = path.file_stem().unwrap().to_str().unwrap().parse().unwrap();
        assert!(!live(pid), "Exec loop {pid} survived cleanup");
    }
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|ext| ext == "escaped")));
}
