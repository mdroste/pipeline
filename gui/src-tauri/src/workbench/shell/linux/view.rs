//! A private FUSE view masks paths that do not yet exist, without asking
//! Bubblewrap to create mountpoints in the project. The host serves only the
//! authorized directory. The trusted child mounts the view in a nested user
//! namespace, closes every control fd, and drops capabilities before seccomp.
//! Ancestor namespaces' /proc roots cannot expose the underlying bind mount.
use super::filesystem::Filtered;
use mxc_sdk::v1::ContainerRequest;
use std::{
    ffi::CString,
    fs::{File, OpenOptions},
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        unix::{
            fs::OpenOptionsExt,
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
const SOCKET: &str = "PIPELINE_COMMAND_VIEW_SOCKET";
const ROOT: &str = "PIPELINE_COMMAND_VIEW_ROOT";

pub(in crate::workbench::shell) struct View {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl View {
    pub(in crate::workbench::shell) fn prepare(
        request: &mut ContainerRequest,
    ) -> Result<Self, String> {
        Self::prepare_inner(request)
            .map_err(|e| format!("Could not prepare the isolated command folder: {e}"))
    }
    fn prepare_inner(request: &mut ContainerRequest) -> io::Result<Self> {
        let root = PathBuf::from(
            request
                .working_directory
                .as_ref()
                .ok_or_else(|| io::Error::other("Missing command folder"))?,
        );
        if root.parent().is_none() {
            return Err(io::Error::other(
                "Choose a command folder below the filesystem root",
            ));
        }
        let fs = request
            .filesystem
            .as_mut()
            .ok_or_else(|| io::Error::other("Missing command filesystem policy"))?;
        let scratch = PathBuf::from(
            fs.readwrite_paths
                .first()
                .ok_or_else(|| io::Error::other("Missing private scratch folder"))?,
        );
        let socket = scratch.join("filesystem.sock");
        let mut readonly: Vec<_> = fs
            .readonly_paths
            .iter()
            .filter_map(|p| Path::new(p).strip_prefix(&root).ok().map(Path::to_path_buf))
            .collect();
        for name in [".git", ".gitconfig"] {
            if std::fs::symlink_metadata(root.join(name)).is_ok() {
                readonly.push(PathBuf::from(name));
            }
        }
        let writable = fs
            .readwrite_paths
            .iter()
            .filter_map(|p| Path::new(p).strip_prefix(&root).ok().map(Path::to_path_buf))
            .collect();
        let mut denied: Vec<_> = fs
            .denied_paths
            .iter()
            .filter_map(|p| Path::new(p).strip_prefix(&root).ok().map(Path::to_path_buf))
            .collect();
        // Also deny the spelling itself when an existing .env symlink resolved
        // to a separate protected target during policy construction.
        denied.push(PathBuf::from(".env"));
        let view = Filtered::new(&root, writable, readonly, denied)?;
        let device = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
            .open("/dev/fuse")
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!("Linux commands need access to /dev/fuse: {e}"),
                )
            })?;
        let listener = UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker = std::thread::Builder::new()
            .name("pipeline-command-files".into())
            .spawn(move || {
                // PassthroughFs sets umask during FUSE initialization. Give
                // this dedicated worker its own fs_struct so the desktop's
                // umask and working directory cannot be changed by the library.
                check(unsafe { libc::unshare(libc::CLONE_FS) })?;
                let deadline = Instant::now() + Duration::from_secs(15);
                let stream = loop {
                    if stopping.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Command filesystem handshake timed out",
                        ));
                    }
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20))
                        }
                        Err(e) => return Err(e),
                    }
                };
                drop(listener);
                std::fs::remove_file(&socket)?;
                stream.set_write_timeout(Some(Duration::from_secs(1)))?;
                stream.set_read_timeout(Some(Duration::from_millis(100)))?;
                send_fd(&stream, device.as_raw_fd())?;
                drop(device);
                let fuse = loop {
                    if stopping.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "Command filesystem mount timed out",
                        ));
                    }
                    match receive_fd(&stream) {
                        Ok(fd) => break File::from(fd),
                        Err(e)
                            if matches!(
                                e.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                            ) => {}
                        Err(e) => return Err(e),
                    }
                };
                drop(stream);
                serve(fuse, view, &stopping)
            })?;
        // These paths are hidden by the view, including files created later.
        // Remaining denies still apply in Bubblewrap's outer namespace.
        fs.denied_paths.retain(|p| !Path::new(p).starts_with(&root));
        let environment = request.environment.get_or_insert_default();
        environment.push((
            SOCKET.into(),
            scratch
                .join("filesystem.sock")
                .to_string_lossy()
                .into_owned(),
        ));
        environment.push((ROOT.into(), root.to_string_lossy().into_owned()));
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
    pub(in crate::workbench::shell) fn finish(&mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| "Command filesystem server failed".to_owned())?
                .map_err(|e| format!("Command filesystem server failed: {e}"))?;
        }
        Ok(())
    }
}
impl Drop for View {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

pub(super) fn enter() -> io::Result<()> {
    let socket =
        std::env::var_os(SOCKET).ok_or_else(|| io::Error::other("Missing filesystem socket"))?;
    let root = std::env::var_os(ROOT).ok_or_else(|| io::Error::other("Missing filesystem root"))?;
    let stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let device = receive_fd(&stream)?;
    // This entry point runs before the GUI or an async runtime creates threads.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    check(unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) })?;
    std::fs::write("/proc/self/setgroups", "deny")?;
    std::fs::write("/proc/self/uid_map", format!("{uid} {uid} 1"))?;
    std::fs::write("/proc/self/gid_map", format!("{gid} {gid} 1"))?;
    // FUSE requires the device to be opened by the mounting user namespace.
    // An O_PATH reference avoids exposing a device node in the command's /dev.
    let fuse = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(format!("/proc/self/fd/{}", device.as_raw_fd()))?;
    drop(device);
    let data = CString::new(format!(
        "fd={},rootmode=40000,user_id={uid},group_id={gid},default_permissions",
        fuse.as_raw_fd()
    ))?;
    use std::os::unix::ffi::OsStrExt;
    let target = CString::new(root.as_bytes())?;
    check(unsafe {
        libc::mount(
            c"pipeline".as_ptr(),
            target.as_ptr(),
            c"fuse".as_ptr(),
            libc::MS_NOSUID | libc::MS_NODEV,
            data.as_ptr().cast(),
        )
    })?;
    send_fd(&stream, fuse.as_raw_fd())?;
    drop(fuse);
    drop(stream);
    // A cwd inherited before the overmount still refers to the underlying tree.
    std::env::set_current_dir("/")?;
    std::env::set_current_dir(root)?;
    drop_capabilities()?;
    Ok(())
}
fn check(result: i32) -> io::Result<()> {
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn drop_capabilities() -> io::Result<()> {
    for cap in 0..64 {
        let result = unsafe { libc::prctl(libc::PR_CAPBSET_DROP, cap, 0, 0, 0) };
        if result != 0 && io::Error::last_os_error().raw_os_error() != Some(libc::EINVAL) {
            return Err(io::Error::last_os_error());
        }
    }
    check(unsafe {
        libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        )
    })?;
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let header = Header {
        version: 0x20080522,
        pid: 0,
    };
    let data = [Data {
        effective: 0,
        permitted: 0,
        inheritable: 0,
    }; 2];
    check(unsafe { libc::syscall(libc::SYS_capset, &header, data.as_ptr()) } as i32)
}
fn send_fd(socket: &UnixStream, fd: RawFd) -> io::Result<()> {
    let mut byte = 1u8;
    let mut iov = libc::iovec {
        iov_base: (&mut byte as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 4];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    // Aligned storage is larger than CMSG_SPACE for one descriptor on both ABIs.
    unsafe {
        msg.msg_controllen = libc::CMSG_SPACE(std::mem::size_of::<RawFd>() as u32) as usize;
        let c = libc::CMSG_FIRSTHDR(&msg);
        (*c).cmsg_level = libc::SOL_SOCKET;
        (*c).cmsg_type = libc::SCM_RIGHTS;
        (*c).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<RawFd>() as u32) as usize;
        std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<RawFd>(), fd);
        if libc::sendmsg(socket.as_raw_fd(), &msg, libc::MSG_NOSIGNAL) != 1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
fn receive_fd(socket: &UnixStream) -> io::Result<OwnedFd> {
    let mut byte = 0u8;
    let mut iov = libc::iovec {
        iov_base: (&mut byte as *mut u8).cast(),
        iov_len: 1,
    };
    let mut control = [0usize; 4];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = std::mem::size_of_val(&control);
    let mut fds = Vec::new();
    // Received descriptors are immediately owned, including unexpected extras.
    unsafe {
        let n = libc::recvmsg(socket.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC);
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Filesystem control pipe closed",
            ));
        }
        let mut c = libc::CMSG_FIRSTHDR(&msg);
        while !c.is_null() {
            if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                let count = (*c).cmsg_len.saturating_sub(libc::CMSG_LEN(0) as usize)
                    / std::mem::size_of::<RawFd>();
                for i in 0..count {
                    fds.push(OwnedFd::from_raw_fd(std::ptr::read_unaligned(
                        libc::CMSG_DATA(c).cast::<RawFd>().add(i),
                    )))
                }
            }
            c = libc::CMSG_NXTHDR(&msg, c);
        }
    }
    if byte != 1 || msg.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) != 0 || fds.len() != 1 {
        return Err(io::Error::other("Invalid filesystem descriptor transfer"));
    }
    Ok(fds.pop().unwrap())
}
fn serve(mut fuse: File, view: Filtered, stop: &AtomicBool) -> io::Result<()> {
    use fuse_backend_rs::{
        api::server::Server,
        transport::{FuseBuf, FuseDevWriter, Reader},
    };
    let server = Server::new(view);
    let mut input = vec![0u8; 1024 * 1024 + 4096];
    let mut output = vec![0u8; 1024 * 1024 + 4096];
    while !stop.load(Ordering::SeqCst) {
        let mut poll = libc::pollfd {
            fd: fuse.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut poll, 1, 100) } < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if poll.revents == 0 {
            continue;
        }
        let n = match fuse.read(&mut input) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.raw_os_error() == Some(libc::ENODEV) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(e) => return Err(e),
        };
        let reader: Reader<'_> =
            Reader::from_fuse_buffer(FuseBuf::new(&mut input[..n])).map_err(io::Error::other)?;
        let writer = FuseDevWriter::new(fuse.as_raw_fd(), &mut output).map_err(io::Error::other)?;
        if let Err(e) = server.handle_message(reader, writer.into(), None, None) {
            if stop.load(Ordering::SeqCst) {
                break;
            }
            return Err(io::Error::other(e));
        }
    }
    Ok(())
}
