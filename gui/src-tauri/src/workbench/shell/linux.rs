//! A tiny in-executable guard entered *inside* the SDK's Bubblewrap namespace.
//! It adds an inherited seccomp filter before Bash can run: pathname Unix
//! sockets must not bridge to host daemons, and nested namespaces/io_uring must
//! not evade the namespace or syscall restrictions. No helper download.
use std::os::unix::process::CommandExt;

pub(super) fn command(command: &str) -> Result<(String, std::path::PathBuf), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    let path = executable
        .to_str()
        .ok_or("Invalid command executable path")?;
    #[cfg(not(test))]
    let entry = format!(
        "{} --pipeline-native-command {}",
        quote(path),
        quote(command)
    );
    #[cfg(test)]
    let entry = format!("PIPELINE_TEST_COMMAND={} {} --exact workbench::shell::tests::linux_command_guard_entry --ignored --nocapture", quote(command), quote(path));
    Ok((entry, executable))
}

pub(crate) fn run_if_requested() -> bool {
    let mut args = std::env::args_os();
    args.next();
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--pipeline-native-command")) {
        return false;
    }
    let Some(command) = args.next().and_then(|s| s.into_string().ok()) else {
        std::process::exit(125)
    };
    if args.next().is_some() {
        std::process::exit(125)
    }
    execute(&command)
}

pub(super) fn execute(command: &str) -> ! {
    if let Err(error) = install_filter() {
        eprintln!("Could not restrict command system calls: {error}");
        std::process::exit(125);
    }
    let error = std::process::Command::new("/bin/bash")
        .args(["--noprofile", "--norc", "-c", command])
        .exec();
    eprintln!("Could not start command: {error}");
    std::process::exit(125);
}

fn install_filter() -> std::io::Result<()> {
    const LOAD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
    const EQ: u16 = 0x15;
    const SET: u16 = 0x45;
    const RET: u16 = 0x06;
    const ALLOW: u32 = 0x7fff0000;
    const KILL: u32 = 0x80000000;
    const ERRNO: u32 = 0x00050000;
    #[cfg(target_arch = "x86_64")]
    const ARCH: u32 = 0xc000003e;
    #[cfg(target_arch = "aarch64")]
    const ARCH: u32 = 0xc00000b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("Native command seccomp requires x86_64 or aarch64");
    let instruction = |code, jt, jf, k| libc::sock_filter { code, jt, jf, k };
    let mut filter = vec![
        instruction(LOAD, 0, 0, 4), // seccomp_data.arch
        instruction(EQ, 1, 0, ARCH),
        instruction(RET, 0, 0, KILL),
        instruction(LOAD, 0, 0, 0), // seccomp_data.nr
        instruction(SET, 0, 1, 0x40000000),
        instruction(RET, 0, 0, KILL), // reject x32 ABI
    ];
    for syscall in [libc::SYS_socket, libc::SYS_socketpair] {
        filter.extend([
            instruction(EQ, 0, 3, syscall as u32),
            instruction(LOAD, 0, 0, 16), // args[0]: address family
            instruction(EQ, 0, 1, libc::AF_UNIX as u32),
            instruction(RET, 0, 0, ERRNO | libc::EPERM as u32),
            instruction(LOAD, 0, 0, 0),
        ]);
    }
    for syscall in [
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_open_tree,
        libc::SYS_move_mount,
        libc::SYS_fsopen,
        libc::SYS_fsmount,
        libc::SYS_fsconfig,
        libc::SYS_mount_setattr,
        libc::SYS_setsid,
    ] {
        filter.extend([
            instruction(EQ, 0, 1, syscall as u32),
            instruction(RET, 0, 0, ERRNO | libc::EPERM as u32),
        ]);
    }
    // libc falls back to clone when clone3 is unavailable. Pointer-valued
    // clone3 arguments cannot be safely inspected by classic seccomp BPF.
    filter.extend([
        instruction(EQ, 0, 1, libc::SYS_clone3 as u32),
        instruction(RET, 0, 0, ERRNO | libc::ENOSYS as u32),
        instruction(EQ, 0, 3, libc::SYS_clone as u32),
        instruction(LOAD, 0, 0, 16),
        instruction(
            SET,
            0,
            1,
            (libc::CLONE_NEWUSER
                | libc::CLONE_NEWNS
                | libc::CLONE_NEWNET
                | libc::CLONE_NEWPID
                | libc::CLONE_NEWIPC
                | libc::CLONE_NEWUTS
                | libc::CLONE_NEWCGROUP) as u32,
        ),
        instruction(RET, 0, 0, ERRNO | libc::EPERM as u32),
        instruction(RET, 0, 0, ALLOW),
    ]);
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    // The kernel copies this valid, live instruction array before returning.
    // no_new_privs makes the filter inherited and impossible to remove.
    unsafe {
        if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
            || libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
