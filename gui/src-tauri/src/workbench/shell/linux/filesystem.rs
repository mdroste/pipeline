//! Per-path restrictions around the upstream descriptor-based passthrough FS.
//! Lookup and every mutation check the same policy. Symlinks are resolved by
//! the client kernel inside Bubblewrap; the server never follows their targets.
//! Read-only aliases stay read-only, including through hard links. Ancestors
//! containing protected paths cannot be renamed out of their restrictions.
use fuse_backend_rs::{
    abi::fuse_abi::{stat64, statvfs64, CreateIn, FsOptions, OpenOptions, SetattrValid},
    api::filesystem::{
        Context, DirEntry, Entry, FileSystem, GetxattrReply, ListxattrReply, ZeroCopyReader,
        ZeroCopyWriter,
    },
    passthrough::{CachePolicy, Config, PassthroughFs},
};
use std::{
    collections::HashMap,
    ffi::{CStr, OsStr},
    io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::Duration,
};
mod hardlink;
struct Node {
    paths: Vec<PathBuf>,
    refs: u64,
}
pub struct Filtered {
    inner: PassthroughFs,
    root: std::fs::File,
    nodes: Mutex<HashMap<u64, Node>>,
    readonly: Vec<PathBuf>,
    denied: Vec<PathBuf>,
    writable: Vec<PathBuf>,
    path_bytes: AtomicUsize,
}
fn err(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}
impl Filtered {
    pub fn new(
        root: &Path,
        writable: Vec<PathBuf>,
        readonly: Vec<PathBuf>,
        denied: Vec<PathBuf>,
    ) -> io::Result<Self> {
        let inner = PassthroughFs::new(Config {
            root_dir: root.to_str().ok_or_else(|| err(libc::EINVAL))?.into(),
            do_import: true,
            attr_timeout: Duration::ZERO,
            entry_timeout: Duration::ZERO,
            cache_policy: CachePolicy::Auto,
            writeback: false,
            xattr: false,
            ..Default::default()
        })?;
        let nodes = Mutex::new(HashMap::from([(
            1,
            Node {
                paths: vec![PathBuf::new()],
                refs: 1,
            },
        )]));
        Ok(Self {
            inner,
            root: std::fs::File::open(root)?,
            nodes,
            readonly,
            denied,
            writable,
            path_bytes: AtomicUsize::new(0),
        })
    }
    fn path(&self, ino: u64, name: &CStr) -> io::Result<PathBuf> {
        let bytes = name.to_bytes();
        if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
            return Err(err(libc::EINVAL));
        }
        let nodes = self.nodes.lock().unwrap();
        let node = nodes.get(&ino).ok_or_else(|| err(libc::ESTALE))?;
        Ok(node.paths[0].join(OsStr::from_bytes(bytes)))
    }
    fn readable(&self, path: &Path) -> io::Result<()> {
        if self.denied.iter().any(|p| path.starts_with(p)) {
            Err(err(libc::EACCES))
        } else {
            Ok(())
        }
    }
    fn mutable(&self, path: &Path, move_entry: bool) -> io::Result<()> {
        self.readable(path)?;
        let writes = self
            .writable
            .iter()
            .filter(|p| path.starts_with(p))
            .map(|p| p.components().count())
            .max();
        let reads = self
            .readonly
            .iter()
            .filter(|p| path.starts_with(p))
            .map(|p| p.components().count())
            .max();
        let allowed = writes.is_some_and(|w| reads.is_none_or(|r| w > r));
        if !allowed
            || move_entry
                && self
                    .readonly
                    .iter()
                    .chain(&self.denied)
                    .any(|p| p.starts_with(path))
        {
            Err(err(libc::EROFS))
        } else {
            Ok(())
        }
    }
    fn write_inode(&self, ino: u64) -> io::Result<()> {
        let nodes = self.nodes.lock().unwrap();
        for p in &nodes.get(&ino).ok_or_else(|| err(libc::ESTALE))?.paths {
            self.mutable(p, false)?;
        }
        Ok(())
    }
    fn entry(&self, ctx: &Context, path: PathBuf, entry: Entry) -> io::Result<Entry> {
        let mut nodes = self.nodes.lock().unwrap();
        let known = nodes.get(&entry.inode);
        let new_path = known.is_none_or(|n| !n.paths.contains(&path));
        let bytes = path.as_os_str().as_bytes().len();
        // The model can keep dentries alive. Bound host-side bookkeeping even
        // when the kernel has not yet sent its corresponding forget requests.
        if new_path
            && (nodes.len() >= 65_536
                || known.is_some_and(|n| n.paths.len() >= 64)
                || self
                    .path_bytes
                    .load(Ordering::Relaxed)
                    .saturating_add(bytes)
                    > 64 * 1024 * 1024)
        {
            self.inner.forget(ctx, entry.inode, 1);
            return Err(err(libc::ENOMEM));
        }
        let n = nodes.entry(entry.inode).or_insert_with(|| Node {
            paths: vec![],
            refs: 0,
        });
        if new_path {
            n.paths.push(path);
            self.path_bytes.fetch_add(bytes, Ordering::Relaxed);
        }
        n.refs += 1;
        Ok(entry)
    }
}
impl FileSystem for Filtered {
    type Inode = u64;
    type Handle = u64;
    fn init(&self, capable: FsOptions) -> io::Result<FsOptions> {
        let mut options = self.inner.init(capable)?;
        options.remove(FsOptions::DO_READDIRPLUS | FsOptions::READDIRPLUS_AUTO);
        Ok(options)
    }
    fn destroy(&self) {
        self.inner.destroy()
    }
    fn lookup(&self, ctx: &Context, parent: Self::Inode, name: &CStr) -> io::Result<Entry> {
        let path = self.path(parent, name)?;
        self.readable(&path)?;
        let entry = self.inner.lookup(ctx, parent, name)?;
        self.entry(ctx, path, entry)
    }
    fn forget(&self, ctx: &Context, inode: Self::Inode, count: u64) {
        self.inner.forget(ctx, inode, count);
        let mut nodes = self.nodes.lock().unwrap();
        if inode != 1 {
            if let Some(n) = nodes.get_mut(&inode) {
                n.refs = n.refs.saturating_sub(count);
                if n.refs == 0 {
                    let removed = nodes.remove(&inode).unwrap();
                    let bytes: usize = removed
                        .paths
                        .iter()
                        .map(|p| p.as_os_str().as_bytes().len())
                        .sum();
                    self.path_bytes.fetch_sub(bytes, Ordering::Relaxed);
                }
            }
        }
    }
    fn getattr(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Option<Self::Handle>,
    ) -> io::Result<(stat64, Duration)> {
        self.inner.getattr(ctx, inode, handle)
    }
    fn setattr(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        attr: stat64,
        handle: Option<Self::Handle>,
        valid: SetattrValid,
    ) -> io::Result<(stat64, Duration)> {
        // The passthrough backend implements some metadata operations through
        // /proc/self/fd. Do not let a no-follow symlink setattr resolve its
        // target in the host namespace. Normal followed operations reach the
        // target inode through the client's confined path resolution instead.
        let (metadata, _) = self.inner.getattr(ctx, inode, handle)?;
        if metadata.st_mode & libc::S_IFMT == libc::S_IFLNK {
            return Err(err(libc::EOPNOTSUPP));
        }
        self.write_inode(inode)?;
        self.inner.setattr(ctx, inode, attr, handle, valid)
    }
    fn readlink(&self, ctx: &Context, inode: Self::Inode) -> io::Result<Vec<u8>> {
        self.inner.readlink(ctx, inode)
    }
    fn symlink(
        &self,
        ctx: &Context,
        linkname: &CStr,
        parent: Self::Inode,
        name: &CStr,
    ) -> io::Result<Entry> {
        let path = self.path(parent, name)?;
        self.mutable(&path, false)?;
        let entry = self.inner.symlink(ctx, linkname, parent, name)?;
        self.entry(ctx, path, entry)
    }
    fn mknod(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        name: &CStr,
        mode: u32,
        rdev: u32,
        umask: u32,
    ) -> io::Result<Entry> {
        if mode & libc::S_IFMT != libc::S_IFREG {
            return Err(err(libc::EOPNOTSUPP));
        }
        let path = self.path(inode, name)?;
        self.mutable(&path, false)?;
        let entry = self.inner.mknod(ctx, inode, name, mode, rdev, umask)?;
        self.entry(ctx, path, entry)
    }
    fn mkdir(
        &self,
        ctx: &Context,
        parent: Self::Inode,
        name: &CStr,
        mode: u32,
        umask: u32,
    ) -> io::Result<Entry> {
        let path = self.path(parent, name)?;
        self.mutable(&path, false)?;
        let entry = self.inner.mkdir(ctx, parent, name, mode, umask)?;
        self.entry(ctx, path, entry)
    }
    fn unlink(&self, ctx: &Context, parent: Self::Inode, name: &CStr) -> io::Result<()> {
        let path = self.path(parent, name)?;
        self.mutable(&path, true)?;
        self.inner.unlink(ctx, parent, name)
    }
    fn rmdir(&self, ctx: &Context, parent: Self::Inode, name: &CStr) -> io::Result<()> {
        let path = self.path(parent, name)?;
        self.mutable(&path, true)?;
        self.inner.rmdir(ctx, parent, name)
    }
    fn rename(
        &self,
        ctx: &Context,
        olddir: Self::Inode,
        oldname: &CStr,
        newdir: Self::Inode,
        newname: &CStr,
        flags: u32,
    ) -> io::Result<()> {
        let old = self.path(olddir, oldname)?;
        let new = self.path(newdir, newname)?;
        self.mutable(&old, true)?;
        self.mutable(&new, true)?;
        if flags & !1 != 0 {
            return Err(err(libc::EOPNOTSUPP));
        }
        let mut nodes = self.nodes.lock().unwrap();
        let mut changed = Vec::new();
        let mut bytes = self.path_bytes.load(Ordering::Relaxed);
        for (inode, node) in nodes.iter() {
            for (index, path) in node.paths.iter().enumerate() {
                if let Ok(suffix) = path.strip_prefix(&old) {
                    let replacement = new.join(suffix);
                    bytes = bytes
                        .saturating_sub(path.as_os_str().as_bytes().len())
                        .saturating_add(replacement.as_os_str().as_bytes().len());
                    if bytes > 64 * 1024 * 1024 {
                        return Err(err(libc::ENOMEM));
                    }
                    changed.push((*inode, index, replacement));
                }
            }
        }
        self.inner
            .rename(ctx, olddir, oldname, newdir, newname, flags)?;
        for (inode, index, path) in changed {
            nodes.get_mut(&inode).unwrap().paths[index] = path;
        }
        self.path_bytes.store(bytes, Ordering::Relaxed);
        Ok(())
    }
    fn link(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        newparent: Self::Inode,
        newname: &CStr,
    ) -> io::Result<Entry> {
        self.write_inode(inode)?;
        let path = self.path(newparent, newname)?;
        self.mutable(&path, false)?;
        let nodes = self.nodes.lock().unwrap();
        let source = &nodes.get(&inode).ok_or_else(|| err(libc::ESTALE))?.paths[0];
        let parent = &nodes
            .get(&newparent)
            .ok_or_else(|| err(libc::ESTALE))?
            .paths[0];
        let (source_stat, _) = self.inner.getattr(ctx, inode, None)?;
        let (parent_stat, _) = self.inner.getattr(ctx, newparent, None)?;
        hardlink::link(
            &self.root,
            source,
            parent,
            newname,
            &source_stat,
            &parent_stat,
        )?;
        drop(nodes);
        let entry = self.inner.lookup(ctx, newparent, newname)?;
        self.entry(ctx, path, entry)
    }
    fn open(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        flags: u32,
        fuse_flags: u32,
    ) -> io::Result<(Option<Self::Handle>, OpenOptions, Option<u32>)> {
        if flags as i32 & (libc::O_ACCMODE | libc::O_TRUNC | libc::O_APPEND) != libc::O_RDONLY {
            self.write_inode(inode)?;
        }
        let (attr, _) = self.inner.getattr(ctx, inode, None)?;
        if attr.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(err(libc::EOPNOTSUPP));
        }
        self.inner.open(ctx, inode, flags, fuse_flags)
    }
    fn create(
        &self,
        ctx: &Context,
        parent: Self::Inode,
        name: &CStr,
        args: CreateIn,
    ) -> io::Result<(Entry, Option<Self::Handle>, OpenOptions, Option<u32>)> {
        let path = self.path(parent, name)?;
        self.mutable(&path, false)?;
        let (entry, handle, options, passthrough) = self.inner.create(ctx, parent, name, args)?;
        match self.entry(ctx, path, entry) {
            Ok(entry) => Ok((entry, handle, options, passthrough)),
            Err(error) => {
                if let Some(handle) = handle {
                    let _ = self.inner.release(
                        ctx,
                        entry.inode,
                        args.flags,
                        handle,
                        false,
                        false,
                        None,
                    );
                }
                Err(error)
            }
        }
    }
    fn read(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        w: &mut dyn ZeroCopyWriter,
        size: u32,
        offset: u64,
        lock_owner: Option<u64>,
        flags: u32,
    ) -> io::Result<usize> {
        self.inner
            .read(ctx, inode, handle, w, size, offset, lock_owner, flags)
    }
    fn write(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        r: &mut dyn ZeroCopyReader,
        size: u32,
        offset: u64,
        lock_owner: Option<u64>,
        delayed_write: bool,
        flags: u32,
        fuse_flags: u32,
    ) -> io::Result<usize> {
        self.write_inode(inode)?;
        self.inner.write(
            ctx,
            inode,
            handle,
            r,
            size,
            offset,
            lock_owner,
            delayed_write,
            flags,
            fuse_flags,
        )
    }
    fn flush(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        lock_owner: u64,
    ) -> io::Result<()> {
        self.inner.flush(ctx, inode, handle, lock_owner)
    }
    fn release(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        flags: u32,
        handle: Self::Handle,
        flush: bool,
        flock_release: bool,
        lock_owner: Option<u64>,
    ) -> io::Result<()> {
        self.inner
            .release(ctx, inode, flags, handle, flush, flock_release, lock_owner)
    }
    fn fsync(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        datasync: bool,
        handle: Self::Handle,
    ) -> io::Result<()> {
        self.inner.fsync(ctx, inode, datasync, handle)
    }
    fn opendir(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        flags: u32,
    ) -> io::Result<(Option<Self::Handle>, OpenOptions)> {
        self.inner.opendir(ctx, inode, flags)
    }
    fn readdir(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        size: u32,
        offset: u64,
        add_entry: &mut dyn FnMut(DirEntry) -> io::Result<usize>,
    ) -> io::Result<()> {
        let nodes = self.nodes.lock().unwrap();
        let base = nodes.get(&inode).ok_or_else(|| err(libc::ESTALE))?.paths[0].clone();
        drop(nodes);
        self.inner
            .readdir(ctx, inode, handle, size, offset, &mut |entry| {
                if self
                    .readable(&base.join(OsStr::from_bytes(entry.name)))
                    .is_err()
                {
                    Ok(1)
                } else {
                    add_entry(entry)
                }
            })
    }
    fn releasedir(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        flags: u32,
        handle: Self::Handle,
    ) -> io::Result<()> {
        self.inner.releasedir(ctx, inode, flags, handle)
    }
    fn fsyncdir(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        datasync: bool,
        handle: Self::Handle,
    ) -> io::Result<()> {
        self.inner.fsyncdir(ctx, inode, datasync, handle)
    }
    fn statfs(&self, ctx: &Context, inode: Self::Inode) -> io::Result<statvfs64> {
        self.inner.statfs(ctx, inode)
    }
    fn getxattr(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        name: &CStr,
        size: u32,
    ) -> io::Result<GetxattrReply> {
        self.inner.getxattr(ctx, inode, name, size)
    }
    fn listxattr(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        size: u32,
    ) -> io::Result<ListxattrReply> {
        self.inner.listxattr(ctx, inode, size)
    }
    fn access(&self, ctx: &Context, inode: Self::Inode, mask: u32) -> io::Result<()> {
        if mask & libc::W_OK as u32 != 0 {
            self.write_inode(inode)?;
        }
        self.inner.access(ctx, inode, mask)
    }
    fn fallocate(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        mode: u32,
        offset: u64,
        length: u64,
    ) -> io::Result<()> {
        self.write_inode(inode)?;
        self.inner
            .fallocate(ctx, inode, handle, mode, offset, length)
    }
    fn lseek(
        &self,
        ctx: &Context,
        inode: Self::Inode,
        handle: Self::Handle,
        offset: u64,
        whence: u32,
    ) -> io::Result<u64> {
        self.inner.lseek(ctx, inode, handle, offset, whence)
    }
}
