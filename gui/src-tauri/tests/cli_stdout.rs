#[cfg(unix)]
#[test]
fn stdout_write_errors_fail_the_command() {
    use std::{
        os::unix::process::CommandExt,
        process::{Command, Stdio},
    };
    let file = tempfile::NamedTempFile::new().unwrap();
    for argument in ["--version", "--help"] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_pipeline-cli"));
        command
            .arg(argument)
            .stdout(Stdio::from(file.reopen().unwrap()))
            .stderr(Stdio::piped());
        // Force EFBIG only in this disposable child. A read-only stdout handle
        // is unsuitable: Rust's standard stdout intentionally ignores EBADF.
        unsafe {
            command.pre_exec(|| {
                libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                let limit = libc::rlimit {
                    rlim_cur: 0,
                    rlim_max: 0,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let output = command.output().unwrap();
        assert!(!output.status.success(), "{argument}: {output:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot write to stdout"));
    }
}
