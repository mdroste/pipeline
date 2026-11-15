use super::super::{policy, process, run};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "requires native Linux isolation, /dev/fuse, and PIPELINE_NATIVE_TEST_HELPER"]
async fn missing_secret_stays_hidden_when_created_after_launch() {
    for writable in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let ready = scratch.path().join("ready");
        std::os::unix::fs::symlink(".env", root.path().join("alias")).unwrap();
        let command = r#"python3 - <<'PY'
import glob, os, pathlib, time
root = pathlib.Path.cwd()
(pathlib.Path(os.environ['TMPDIR']) / 'ready').write_text('ready')
while not (root / 'continue').exists(): time.sleep(.01)
for p in [root / '.env', root / 'alias']:
    try: print(p.read_text())
    except OSError: pass
for proc in glob.glob('/proc/[0-9]*'):
    for base in ['root', 'cwd']:
        p = pathlib.Path(proc) / base / str(root).lstrip('/') / '.env' if base == 'root' else pathlib.Path(proc) / base / '.env'
        try: print(p.read_text())
        except OSError: pass
    for fd in glob.glob(proc + '/fd/*'):
        for p in [pathlib.Path(fd) / '.env', pathlib.Path(fd) / str(root).lstrip('/') / '.env']:
            try: print(p.read_text())
            except OSError: pass
print('protected')
PY"#;
        let request =
            policy::request(root.path(), writable, scratch.path(), command, None).unwrap();
        let task = tokio::spawn(process::run(request, scratch, None));
        tokio::time::timeout(Duration::from_secs(10), async {
            while !ready.exists() {
                assert!(
                    !task.is_finished(),
                    "Command stopped before the creation barrier"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!root.path().join(".env").exists());
        std::fs::write(root.path().join(".env"), "LATE PRIVATE SENTINEL").unwrap();
        std::fs::write(root.path().join("continue"), "continue").unwrap();
        let result = task.await.unwrap().unwrap();
        assert_eq!(result["exitCode"], json!(0), "{result}");
        assert_eq!(result["stdout"], "protected\n", "{result}");
        assert_eq!(
            std::fs::read_to_string(root.path().join(".env")).unwrap(),
            "LATE PRIVATE SENTINEL"
        );
    }
}

#[tokio::test]
#[ignore = "requires native Linux isolation, /dev/fuse, and PIPELINE_NATIVE_TEST_HELPER"]
async fn private_view_preserves_python_file_and_subprocess_operations() {
    let host_umask = || {
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .find(|line| line.starts_with("Umask:"))
            .unwrap()
            .to_owned()
    };
    let before = host_umask();

    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("private.txt");
    std::fs::write(&secret, "outside").unwrap();
    let modified = std::fs::metadata(&secret).unwrap().modified().unwrap();
    std::os::unix::fs::symlink(&secret, root.path().join("escape")).unwrap();
    std::fs::create_dir(root.path().join(".git")).unwrap();
    std::fs::write(root.path().join(".git/config"), "original").unwrap();
    let command = r#"python3 - <<'PY'
import mmap, os, pathlib, shutil, subprocess, tempfile
root = pathlib.Path.cwd()
status = pathlib.Path('/proc/self/status').read_text().splitlines()
for line in status:
    if line.split(':')[0] in ['CapEff', 'CapPrm', 'CapInh', 'CapBnd', 'CapAmb']:
        assert int(line.split(':')[1].strip(), 16) == 0, line
p = root / 'normal.txt'
p.write_bytes(b'abcdef')
with p.open('r+b') as f:
    with mmap.mmap(f.fileno(), 0) as m:
        m[0:3] = b'XYZ'
        m.flush()
    f.truncate(5)
    os.fsync(f.fileno())
p.chmod(0o640)
os.utime(p, (1700000000, 1700000000))
assert p.read_bytes() == b'XYZde'
assert p.stat().st_mode & 0o777 == 0o640
assert int(p.stat().st_mtime) == 1700000000
os.link(p, root / 'hardlink')
assert (root / 'hardlink').read_bytes() == b'XYZde'
(root / 'relative').symlink_to('normal.txt')
assert (root / 'relative').read_bytes() == b'XYZde'
with tempfile.TemporaryDirectory(dir=root) as d:
    (pathlib.Path(d) / 'child').write_text('child')
    renamed = root / 'renamed'
    pathlib.Path(d).rename(renamed)
    assert (renamed / 'child').read_text() == 'child'
    shutil.rmtree(renamed)
for action in [lambda: os.utime(root / 'escape', (1700000000, 1700000000), follow_symlinks=False), lambda: (root / '.git').rename(root / 'git-moved'), lambda: os.link(root / '.git/config', root / 'git-alias'), lambda: (root / '.git/config').write_text('changed'), lambda: p.rename(root / '.env')]:
    try: action()
    except OSError: pass
    else: raise AssertionError('protected mutation succeeded')
result = subprocess.check_output(['python3', '-c', "print('subprocess ok')"], text=True)
assert result == 'subprocess ok\n'
print('file operations ok')
PY"#;
    let result = run(root.path(), true, &[], false, command).await.unwrap();
    assert_eq!(
        host_umask(),
        before,
        "Filesystem server changed the desktop umask"
    );
    assert_eq!(result["exitCode"], json!(0), "{result}");
    assert_eq!(result["stdout"], "file operations ok\n", "{result}");
    assert_eq!(
        std::fs::read(root.path().join("normal.txt")).unwrap(),
        b"XYZde"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join(".git/config")).unwrap(),
        "original"
    );
    assert!(!root.path().join(".env").exists());
    assert_eq!(
        std::fs::metadata(&secret).unwrap().modified().unwrap(),
        modified
    );
    assert!(!root.path().join("git-alias").exists());
}

#[test]
fn filesystem_root_cannot_be_overmounted_as_a_project() {
    let mut request = mxc_sdk::v1::ContainerRequest::new("true");
    request.working_directory = Some("/".into());
    let result = super::View::prepare(&mut request);
    assert!(matches!(result, Err(error) if error.contains("below the filesystem root")));
}
